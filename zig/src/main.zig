const std = @import("std");
const Io = std.Io;
const Dir = std.Io.Dir;
const Writer = std.Io.Writer;
const dropbox = @import("dropbox.zig");

const VERSION = "0.4.0";

// --- macOS C interop (libSystem) -------------------------------------------

extern "c" fn setxattr(path: [*:0]const u8, name: [*:0]const u8, value: ?*const anyopaque, size: usize, position: u32, options: c_int) c_int;
extern "c" fn getxattr(path: [*:0]const u8, name: [*:0]const u8, value: ?*anyopaque, size: usize, position: u32, options: c_int) isize;

const XATTR_TIMEMACHINE: [*:0]const u8 = "com.apple.metadata:com_apple_backup_excludeItem";
const XATTR_DROPBOX: [*:0]const u8 = "com.dropbox.ignored";

// --- matchers ---------------------------------------------------------------

/// Directory name -> sibling marker files that confirm it's a build/dep dir.
const Matcher = struct { dir: []const u8, markers: []const []const u8 };

const matchers = [_]Matcher{
    .{ .dir = "bower_components", .markers = &.{"bower.json"} },
    .{ .dir = "node_modules", .markers = &.{"package.json"} },
    .{ .dir = ".next", .markers = &.{"next.config.mjs"} },
    .{ .dir = ".swc", .markers = &.{"next.config.mjs"} },
    .{ .dir = ".turbo", .markers = &.{"turbo.json"} },
    .{ .dir = ".nx", .markers = &.{"nx.json"} },
    .{ .dir = "target", .markers = &.{ "Cargo.toml", "pox.xml" } },
    .{ .dir = "zig-out", .markers = &.{"build.zig"} },
    .{ .dir = "zig-cache", .markers = &.{"build.zig"} },
    .{ .dir = ".zig-cache", .markers = &.{"build.zig"} },
    .{ .dir = ".build", .markers = &.{"Package.swift"} },
    .{ .dir = "Pods", .markers = &.{"Podfile"} },
    .{ .dir = "vendor", .markers = &.{"go.mod"} },
    .{ .dir = ".godot", .markers = &.{"project.godot"} },
    .{ .dir = "_work", .markers = &.{".runner"} },
};

/// Directory names skipped entirely during the Time Machine walk.
const tm_exclude = [_][]const u8{ "Library", ".Trash", "tmp" };

fn markersFor(name: []const u8) ?[]const []const u8 {
    for (matchers) |m| {
        if (std.mem.eql(u8, m.dir, name)) return m.markers;
    }
    return null;
}

fn inList(name: []const u8, list: []const []const u8) bool {
    for (list) |e| {
        if (std.mem.eql(u8, e, name)) return true;
    }
    return false;
}

// --- options & stats --------------------------------------------------------

const Options = struct {
    verbose: bool = false,
    dry_run: bool = false,
    show_immutable: bool = false,
    tm_skip_dropbox: bool = false,
    dont_sync_dropbox: bool = false,
    help: bool = false,
    version: bool = false,
    path: ?[]const u8 = null,
};

const Stats = struct {
    matched: u64 = 0,
    skipped: u64 = 0,
    added: u64 = 0,
    immutable: u64 = 0,
};

// --- xattr helpers ----------------------------------------------------------

fn alreadyExcluded(attr: [*:0]const u8, path: [*:0]const u8) bool {
    var buf: [4]u8 = undefined;
    return getxattr(path, attr, &buf, buf.len, 0, 0) >= 0;
}

fn exclude(attr: [*:0]const u8, path: [*:0]const u8) void {
    const value = [_]u8{1};
    _ = setxattr(path, attr, &value, value.len, 0, 0);
}

// --- walk -------------------------------------------------------------------

fn fileExists(io: Io, dir: Dir, name: []const u8) bool {
    _ = dir.statFile(io, name, .{}) catch return false;
    return true;
}

/// Whether a marker file can be opened for writing — the proxy for "this dir is
/// not locked/immutable", matching the Rust is_writeable check.
fn fileWriteable(io: Io, dir: Dir, name: []const u8) bool {
    var f = dir.openFile(io, name, .{ .mode = .read_write }) catch return false;
    f.close(io);
    return true;
}

const WalkCtx = struct {
    io: Io,
    alloc: std.mem.Allocator,
    out: *Writer,
    attr: [*:0]const u8,
    exclusions: []const []const u8,
    opts: Options,
    stats: *Stats,
};

fn walk(ctx: *WalkCtx, dir: Dir, dir_path: []const u8) !void {
    var it = dir.iterate();
    while (true) {
        const entry = (it.next(ctx.io) catch break) orelse break;
        if (entry.kind != .directory) continue;
        if (inList(entry.name, ctx.exclusions)) continue; // skip, don't recurse

        const child_path = try std.fs.path.join(ctx.alloc, &.{ dir_path, entry.name });

        if (markersFor(entry.name)) |markers| {
            if (try handleMatch(ctx, dir, dir_path, child_path, markers)) continue;
        }

        var child = dir.openDir(ctx.io, entry.name, .{ .iterate = true }) catch continue;
        defer child.close(ctx.io);
        try walk(ctx, child, child_path);
    }
}

/// Returns true if the directory was handled (excluded or counted immutable),
/// meaning the caller should not descend into it.
fn handleMatch(ctx: *WalkCtx, dir: Dir, dir_path: []const u8, child_path: []const u8, markers: []const []const u8) !bool {
    for (markers) |marker| {
        if (!fileExists(ctx.io, dir, marker)) continue;

        if (!fileWriteable(ctx.io, dir, marker)) {
            if (ctx.opts.show_immutable) {
                const mpath = try std.fs.path.join(ctx.alloc, &.{ dir_path, marker });
                try ctx.out.print("  ^ {s}\n", .{mpath});
            }
            ctx.stats.immutable += 1;
            continue; // a different marker might still be writeable
        }

        ctx.stats.matched += 1;
        const cpath = try ctx.alloc.dupeZ(u8, child_path);
        if (alreadyExcluded(ctx.attr, cpath)) {
            ctx.stats.skipped += 1;
        } else {
            ctx.stats.added += 1;
            if (!ctx.opts.dry_run) exclude(ctx.attr, cpath);
            if (ctx.opts.verbose or ctx.opts.dry_run) {
                try ctx.out.print("  + {s}\n", .{child_path});
            }
        }
        return true; // one marker is enough; do not traverse deeper
    }
    return false;
}

// --- arg parsing & help -----------------------------------------------------

const ParseResult = union(enum) {
    ok: Options,
    unknown_flag: []const u8,
    missing_value: []const u8,
};

fn parseArgs(args: std.process.Args) ParseResult {
    var opts = Options{};
    var it = std.process.Args.Iterator.init(args);
    _ = it.skip(); // argv[0]
    while (it.next()) |arg| {
        if (std.mem.eql(u8, arg, "--verbose") or std.mem.eql(u8, arg, "-v")) {
            opts.verbose = true;
        } else if (std.mem.eql(u8, arg, "--dry-run")) {
            opts.dry_run = true;
        } else if (std.mem.eql(u8, arg, "--show-immutable")) {
            opts.show_immutable = true;
        } else if (std.mem.eql(u8, arg, "--tm-skip-dropbox")) {
            opts.tm_skip_dropbox = true;
        } else if (std.mem.eql(u8, arg, "--dont-sync-dropbox")) {
            opts.dont_sync_dropbox = true;
        } else if (std.mem.eql(u8, arg, "--help") or std.mem.eql(u8, arg, "-h")) {
            opts.help = true;
        } else if (std.mem.eql(u8, arg, "--version") or std.mem.eql(u8, arg, "-V")) {
            opts.version = true;
        } else if (std.mem.eql(u8, arg, "--path") or std.mem.eql(u8, arg, "-p")) {
            opts.path = it.next() orelse return .{ .missing_value = arg };
        } else {
            return .{ .unknown_flag = arg };
        }
    }
    return .{ .ok = opts };
}

fn printHelp(out: *Writer) !void {
    try out.writeAll(
        \\Exclude package dependency and build directories from Time Machine backups and Dropbox sync
        \\
        \\Usage: morlock [OPTIONS]
        \\
        \\Options:
        \\  -v, --verbose            Print each directory as it is processed
        \\  -p, --path <PATH>        Scan this directory instead of $HOME
        \\      --dry-run            Report what would be excluded without changing anything
        \\      --show-immutable     Report directories whose marker file is not writeable
        \\      --tm-skip-dropbox    Skip the Dropbox tree during the Time Machine walk
        \\      --dont-sync-dropbox  Do not apply Dropbox ignore attributes
        \\  -h, --help               Print help
        \\  -V, --version            Print version
        \\
    );
}

/// Print a clap-style usage error to stderr and exit with code 2.
fn usageError(comptime fmt: []const u8, args: anytype) noreturn {
    std.debug.print("error: " ++ fmt ++ "\n", args);
    std.debug.print("\nUsage: morlock [OPTIONS]\nFor more information, try '--help'.\n", .{});
    std.process.exit(2);
}

// --- main -------------------------------------------------------------------

pub fn main(init: std.process.Init) !void {
    const io = init.io;
    const alloc = init.arena.allocator();

    const opts = switch (parseArgs(init.minimal.args)) {
        .ok => |o| o,
        .unknown_flag => |f| usageError("unrecognized argument '{s}'", .{f}),
        .missing_value => |f| usageError("a value is required for '{s}'", .{f}),
    };

    // Buffered stdout via the 0.16 std.Io writer.
    var out_buf: [4096]u8 = undefined;
    var out_fw = std.Io.File.stdout().writer(io, &out_buf);
    const out = &out_fw.interface;
    defer out.flush() catch {};

    if (opts.help) return printHelp(out);
    if (opts.version) return out.print("morlock {s}\n", .{VERSION});

    const home = init.environ_map.get("HOME") orelse return error.NoHomeDir;
    const root_path = opts.path orelse home;

    const dbx_folder = try dropbox.resolveFolder(io, alloc, home);
    const has_dropbox = dbx_folder != null;

    if (opts.dry_run) try out.print("(dry run \u{2014} no attributes will be modified)\n", .{});

    // Build the Time Machine exclusion list (statics + optionally Dropbox).
    var excl_buf: [tm_exclude.len + 1][]const u8 = undefined;
    @memcpy(excl_buf[0..tm_exclude.len], &tm_exclude);
    var excl_len: usize = tm_exclude.len;
    if (has_dropbox and opts.tm_skip_dropbox) {
        excl_buf[excl_len] = std.fs.path.basename(dbx_folder.?);
        excl_len += 1;
    }
    const exclusions = excl_buf[0..excl_len];

    // Time Machine walk.
    if (opts.verbose) {
        try out.print("- Excluding package dependencies from Time Machine\n", .{});
        try out.print("  - From {s}\n", .{root_path});
    }
    var tmstats = Stats{};
    {
        var tm_ctx = WalkCtx{ .io = io, .alloc = alloc, .out = out, .attr = XATTR_TIMEMACHINE, .exclusions = exclusions, .opts = opts, .stats = &tmstats };
        var root = try Dir.cwd().openDir(io, root_path, .{ .iterate = true });
        defer root.close(io);
        try walk(&tm_ctx, root, root_path);
    }
    if (opts.verbose) {
        try out.print("  % checked {d}, skipped {d}, added {d}, immutable: {d}\n", .{ tmstats.matched, tmstats.skipped, tmstats.added, tmstats.immutable });
    }

    // Dropbox sync walk.
    if (has_dropbox and !opts.dont_sync_dropbox) {
        const dpath = dbx_folder.?;
        if (opts.verbose) {
            try out.print("\n- Excluding package dependencies from Dropbox Sync\n", .{});
            try out.print("  - From {s}\n", .{dpath});
        }
        var dstats = Stats{};
        if (Dir.cwd().openDir(io, dpath, .{ .iterate = true })) |droot_const| {
            var droot = droot_const;
            defer droot.close(io);
            var dbx_ctx = WalkCtx{ .io = io, .alloc = alloc, .out = out, .attr = XATTR_DROPBOX, .exclusions = &.{}, .opts = opts, .stats = &dstats };
            try walk(&dbx_ctx, droot, dpath);
        } else |_| {}
        if (opts.verbose) {
            try out.print("  % checked {d}, skipped {d}, added {d}\n", .{ dstats.matched, dstats.skipped, dstats.added });
        }
    }
}

// --- tests ------------------------------------------------------------------

test {
    std.testing.refAllDecls(dropbox);
}

test "markersFor maps known dirs to their markers" {
    try std.testing.expect(markersFor("node_modules") != null);
    try std.testing.expectEqualStrings("package.json", markersFor("node_modules").?[0]);
    try std.testing.expectEqualStrings("turbo.json", markersFor(".turbo").?[0]);
    try std.testing.expect(markersFor("target").?.len == 2);
    try std.testing.expect(markersFor("not_a_build_dir") == null);
}

test "inList covers the time machine skip list" {
    try std.testing.expect(inList("Library", &tm_exclude));
    try std.testing.expect(inList(".Trash", &tm_exclude));
    try std.testing.expect(!inList("src", &tm_exclude));
}

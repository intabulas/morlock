const std = @import("std");
const Io = std.Io;
const Dir = std.Io.Dir;

// --- macOS C interop (libSystem) -------------------------------------------

extern "c" fn setxattr(path: [*:0]const u8, name: [*:0]const u8, value: ?*const anyopaque, size: usize, position: u32, options: c_int) c_int;
extern "c" fn getxattr(path: [*:0]const u8, name: [*:0]const u8, value: ?*anyopaque, size: usize, position: u32, options: c_int) isize;

const XATTR_TIMEMACHINE: [*:0]const u8 = "com.apple.metadata:com_apple_backup_excludeItem";

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

const tm_exclude = [_][]const u8{ "Library", ".Trash", "tmp" };

fn markersFor(name: []const u8) ?[]const []const u8 {
    for (matchers) |m| {
        if (std.mem.eql(u8, m.dir, name)) return m.markers;
    }
    return null;
}

fn isExcluded(name: []const u8) bool {
    for (tm_exclude) |e| {
        if (std.mem.eql(u8, e, name)) return true;
    }
    return false;
}

// --- options & stats --------------------------------------------------------

const Options = struct {
    verbose: bool = false,
    dry_run: bool = false,
    show_immutable: bool = false,
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

fn walk(io: Io, alloc: std.mem.Allocator, dir: Dir, dir_path: []const u8, opts: Options, stats: *Stats) !void {
    var it = dir.iterate();
    while (true) {
        const entry = (it.next(io) catch break) orelse break;
        if (entry.kind != .directory) continue;
        if (isExcluded(entry.name)) continue; // skip, do not recurse

        const child_path = try std.fs.path.join(alloc, &.{ dir_path, entry.name });

        if (markersFor(entry.name)) |markers| {
            var matched_marker = false;
            for (markers) |marker| {
                if (fileExists(io, dir, marker)) {
                    matched_marker = true;
                    break;
                }
            }
            if (matched_marker) {
                stats.matched += 1;
                const cpath = try alloc.dupeZ(u8, child_path);
                if (alreadyExcluded(XATTR_TIMEMACHINE, cpath)) {
                    stats.skipped += 1;
                } else {
                    stats.added += 1;
                    if (!opts.dry_run) exclude(XATTR_TIMEMACHINE, cpath);
                    if (opts.verbose or opts.dry_run) {
                        std.debug.print("  + {s}\n", .{child_path});
                    }
                }
                continue; // matched: do not traverse deeper
            }
        }

        var child = dir.openDir(io, entry.name, .{ .iterate = true }) catch continue;
        defer child.close(io);
        try walk(io, alloc, child, child_path, opts, stats);
    }
}

// --- arg parsing ------------------------------------------------------------

fn parseArgs(args: std.process.Args) Options {
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
        } else if (std.mem.eql(u8, arg, "--path") or std.mem.eql(u8, arg, "-p")) {
            opts.path = it.next();
        }
    }
    return opts;
}

// --- main -------------------------------------------------------------------

pub fn main(init: std.process.Init) !void {
    const io = init.io;
    const alloc = init.arena.allocator();

    const opts = parseArgs(init.minimal.args);

    const home = init.environ_map.get("HOME") orelse return error.NoHomeDir;
    const root_path = opts.path orelse home;

    if (opts.dry_run) std.debug.print("(dry run \u{2014} no attributes will be modified)\n", .{});
    if (opts.verbose) {
        std.debug.print("- Excluding package dependencies from Time Machine\n", .{});
        std.debug.print("  - From {s}\n", .{root_path});
    }

    var root = try Dir.cwd().openDir(io, root_path, .{ .iterate = true });
    defer root.close(io);

    var stats = Stats{};
    try walk(io, alloc, root, root_path, opts, &stats);

    if (opts.verbose) {
        std.debug.print("  % checked {d}, skipped {d}, added {d}, immutable: {d}\n", .{ stats.matched, stats.skipped, stats.added, stats.immutable });
    }
}

// --- tests ------------------------------------------------------------------

test "markersFor maps known dirs to their markers" {
    try std.testing.expect(markersFor("node_modules") != null);
    try std.testing.expectEqualStrings("package.json", markersFor("node_modules").?[0]);
    try std.testing.expectEqualStrings("turbo.json", markersFor(".turbo").?[0]);
    try std.testing.expect(markersFor("target").?.len == 2);
    try std.testing.expect(markersFor("not_a_build_dir") == null);
}

test "isExcluded covers the time machine skip list" {
    try std.testing.expect(isExcluded("Library"));
    try std.testing.expect(isExcluded(".Trash"));
    try std.testing.expect(!isExcluded("src"));
}

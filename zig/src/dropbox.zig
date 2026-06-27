const std = @import("std");
const Io = std.Io;
const Dir = std.Io.Dir;

const HOST_DB = ".dropbox/host.db";
const MAESTRAL_INI = "Library/Application Support/maestral/maestral.ini";

/// Resolve the Dropbox sync folder, or null if no Dropbox/Maestral install is
/// found. Mirrors the Rust dropbox.rs: the official client stores a
/// base64-encoded path on the 2nd line of host.db; Maestral stores it as the
/// `[sync] path` key in maestral.ini.
pub fn resolveFolder(io: Io, alloc: std.mem.Allocator, home: []const u8) !?[]const u8 {
    const cwd = Dir.cwd();

    const host_db = try std.fs.path.join(alloc, &.{ home, HOST_DB });
    if (cwd.readFileAlloc(io, host_db, alloc, std.Io.Limit.limited(64 * 1024))) |contents| {
        return parseHostDb(alloc, contents);
    } else |_| {}

    const ini = try std.fs.path.join(alloc, &.{ home, MAESTRAL_INI });
    if (cwd.readFileAlloc(io, ini, alloc, std.Io.Limit.limited(1024 * 1024))) |contents| {
        return parseMaestralIni(contents);
    } else |_| {}

    return null;
}

fn parseHostDb(alloc: std.mem.Allocator, contents: []const u8) !?[]const u8 {
    var lines = std.mem.splitScalar(u8, contents, '\n');
    _ = lines.next(); // first line is ignored
    const encoded = lines.next() orelse return null;
    const trimmed = std.mem.trim(u8, encoded, " \r\n");
    if (trimmed.len == 0) return null;

    const dec = std.base64.standard.Decoder;
    const n = dec.calcSizeForSlice(trimmed) catch return null;
    const buf = try alloc.alloc(u8, n);
    dec.decode(buf, trimmed) catch return null;
    return buf;
}

fn parseMaestralIni(contents: []const u8) ?[]const u8 {
    var in_sync = false;
    var lines = std.mem.splitScalar(u8, contents, '\n');
    while (lines.next()) |raw| {
        const line = std.mem.trim(u8, raw, " \t\r");
        if (line.len == 0 or line[0] == ';' or line[0] == '#') continue;
        if (line[0] == '[') {
            in_sync = std.mem.eql(u8, line, "[sync]");
            continue;
        }
        if (!in_sync) continue;
        const eq = std.mem.indexOfScalar(u8, line, '=') orelse continue;
        const key = std.mem.trim(u8, line[0..eq], " \t");
        if (std.mem.eql(u8, key, "path")) {
            return std.mem.trim(u8, line[eq + 1 ..], " \t");
        }
    }
    return null;
}

test "parseHostDb decodes the base64 second line" {
    const path = "/Users/me/Dropbox";
    var enc_buf: [64]u8 = undefined;
    const enc = std.base64.standard.Encoder.encode(&enc_buf, path);
    var line_buf: [128]u8 = undefined;
    const contents = try std.fmt.bufPrint(&line_buf, "ignored\n{s}\n", .{enc});
    const got = try parseHostDb(std.testing.allocator, contents);
    defer if (got) |g| std.testing.allocator.free(g);
    try std.testing.expectEqualStrings(path, got.?);
}

test "parseMaestralIni reads the sync path" {
    const ini = "[main]\nfoo = bar\n[sync]\npath = /Users/me/Dropbox\n";
    try std.testing.expectEqualStrings("/Users/me/Dropbox", parseMaestralIni(ini).?);
}

test "parseMaestralIni returns null without a sync path" {
    try std.testing.expect(parseMaestralIni("[main]\nfoo = bar\n") == null);
}

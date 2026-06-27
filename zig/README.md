# morlock (Zig port)

An experimental Zig port of morlock, living alongside the canonical Rust
implementation in the parent directory. Built and tested against **Zig 0.16**.

## Why

Mostly for fun, and because the core of morlock is a directory walk plus a few
macOS `xattr` syscalls — a good fit for Zig with minimal dependencies (just
libc for the xattr/`setxattr` calls; no package manager involved).

## Build & run

```sh
zig build            # produces zig-out/bin/morlock
zig build run -- --path ~/code --dry-run --verbose
zig build test       # unit tests
```

## What's ported

- Recursive home/`--path` walk with **pruning** (matched/excluded dirs are not
  descended into), via a hand-rolled recursion over `std.Io.Dir.iterate`
- The full matcher table (Node, Next.js, Turborepo, Nx, Rust, Zig, Swift, Go,
  Godot, GitHub runner) with the sibling-marker check
- The Time Machine skip list (`Library`, `.Trash`, `tmp`)
- Setting / checking the Time Machine exclusion xattr via libc `setxattr` /
  `getxattr` (the extra macOS `position`/`options` params are handled)
- Flags: `--path`/`-p`, `--dry-run`, `--verbose`/`-v`, `--show-immutable` (parsed)

## Not yet ported (vs. the Rust version)

- **Dropbox / Maestral** resolution and the Dropbox sync walk (the `host.db`
  base64 + `maestral.ini` parsing), and `--tm-skip-dropbox` /
  `--dont-sync-dropbox`
- The **writeability / immutable** check (`--show-immutable` is parsed but not
  yet acted on)
- `--help` / `--version` text and clap-style usage errors

## Notes on Zig 0.16

This targets the post-`std.Io` reorganization. A few things that differ from
older tutorials:

- `main(init: std.process.Init)` is the entry point — std provides the `Io`
  instance, an arena, a gpa, args, and the environment map.
- Filesystem calls live on `std.Io.Dir` and take the `Io` instance explicitly.
- `std.os.argv` is gone; args come from `init.minimal.args`.

//! End-to-end tests that run the built binary against a throwaway tree.
//!
//! Gated to macOS: they set the real Time Machine exclusion xattr, which only
//! behaves meaningfully there. Every invocation passes `--dont-sync-dropbox`
//! so the run is confined to the temp `--path` and never touches a real
//! Dropbox/Maestral folder on the host.
#![cfg(target_os = "macos")]

use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const TM_ATTR: &str = "com.apple.metadata:com_apple_backup_excludeItem";

fn morlock(root: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_morlock"));
    cmd.arg("--dont-sync-dropbox").arg("--path").arg(root);
    cmd
}

/// A project with two excludable dirs (each next to a marker file), plus a
/// `node_modules` with *no* sibling marker that must be left alone.
fn fixture() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("proj/node_modules")).unwrap();
    std::fs::create_dir_all(root.join("proj/target")).unwrap();
    std::fs::create_dir_all(root.join("nomatch/node_modules")).unwrap();
    std::fs::write(root.join("proj/package.json"), "{}").unwrap();
    std::fs::write(root.join("proj/Cargo.toml"), "").unwrap();
    dir
}

fn is_excluded(path: &Path) -> bool {
    xattr::get(path, TM_ATTR).unwrap().is_some()
}

#[test]
fn dry_run_makes_no_changes() {
    let dir = fixture();
    let root = dir.path();

    let status = morlock(root).arg("--dry-run").status().unwrap();
    assert!(status.success());

    assert!(!is_excluded(&root.join("proj/node_modules")));
    assert!(!is_excluded(&root.join("proj/target")));
}

#[test]
fn real_run_excludes_only_dirs_with_a_sibling_marker() {
    let dir = fixture();
    let root = dir.path();

    assert!(morlock(root).status().unwrap().success());

    // Dirs sitting next to their marker file get excluded.
    assert!(is_excluded(&root.join("proj/node_modules")));
    assert!(is_excluded(&root.join("proj/target")));
    // node_modules with no sibling package.json is left untouched.
    assert!(!is_excluded(&root.join("nomatch/node_modules")));
}

#[test]
fn second_run_is_idempotent() {
    let dir = fixture();
    let root = dir.path();

    assert!(morlock(root).status().unwrap().success());
    assert!(morlock(root).status().unwrap().success());

    // Still excluded, and the run did not error on the already-excluded dir.
    assert!(is_excluded(&root.join("proj/target")));
}

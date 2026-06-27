use anyhow::{Context, Result};
use clap::Parser;
use dropbox::DropBox;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use xattr::{get, set};
mod dropbox;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    verbose: bool,
    #[arg(long)]
    tm_skip_dropbox: bool,
    #[arg(long)]
    dont_sync_dropbox: bool,
    #[arg(short, long)]
    path: Option<String>,
    #[arg(long)]
    show_immutable: bool,
    /// Report what would be excluded without modifying any attributes.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Default)]
struct Stats {
    matched: u64,
    skipped: u64,
    added: u64,
    immutable: u64,
}

struct WalkOptions<'a> {
    pub directory: &'a Path,
    pub exclusions: &'a [&'a str],
    pub matchers: &'a HashMap<&'a str, Vec<&'a str>>,
    pub attribute: &'a str,
    pub root_path: &'a str,
    pub verbose: bool,
    pub show_immutable: bool,
    pub dry_run: bool,
}

const XATTR_DROPBOX: &str = "com.dropbox.ignored";
const XATTR_TIMEMACHINE: &str = "com.apple.metadata:com_apple_backup_excludeItem";

/// Directory name -> sibling marker files that confirm it's a build/dependency dir.
fn build_matchers() -> HashMap<&'static str, Vec<&'static str>> {
    HashMap::from([
        ("bower_components", vec!["bower.json"]),
        ("node_modules", vec!["package.json"]),
        ("target", vec!["Cargo.toml", "pox.xml"]),
        ("Pods", vec!["Podfile"]),
        ("vendor", vec!["go.mod"]),
        ("_work", vec![".runner"]),
        (".godot", vec!["project.godot"]),
        (".next", vec!["next.config.mjs"]),
        (".swc", vec!["next.config.mjs"]),
    ])
}

fn main() -> Result<()> {
    let args = Args::parse();

    let matchers = build_matchers();

    let homedir = dirs::home_dir().context("could not determine home directory")?;
    let homedir_str = homedir
        .to_str()
        .context("home directory path is not valid UTF-8")?;

    // Root of the Time Machine scan (defaults to $HOME). Also the prefix that
    // gets shortened to "~" in output.
    let starting_path = match args.path.as_deref() {
        Some(p) if !p.is_empty() => p,
        _ => homedir_str,
    };
    let scan_root = PathBuf::from(starting_path);

    if args.dry_run {
        println!("(dry run \u{2014} no attributes will be modified)");
    }

    let mut dbx = DropBox::new();
    // folder() populates dbx.path, which name() reads, so it must run first.
    dbx.folder()?;
    let has_dropbox = !dbx.path.is_empty();
    let dbxname = dbx.name().to_string();

    // Directory names we skip entirely during the Time Machine walk.
    let mut tm_exclude = vec!["Library", ".Trash", "tmp"];
    // If asked, skip the Dropbox tree under Time Machine (Dropbox handles its own).
    if has_dropbox && args.tm_skip_dropbox {
        tm_exclude.push(&dbxname);
    }

    if args.verbose {
        println!("- Excluding package dependencies from Time Machine");
        println!("  - From {starting_path}");
    }

    let mut tmstats = Stats::default();
    walk(
        WalkOptions {
            directory: &scan_root,
            exclusions: &tm_exclude,
            matchers: &matchers,
            attribute: XATTR_TIMEMACHINE,
            root_path: starting_path,
            verbose: args.verbose,
            show_immutable: args.show_immutable,
            dry_run: args.dry_run,
        },
        &mut tmstats,
    );

    if args.verbose {
        println!(
            "  % checked {}, skipped {}, added {}, immutable: {}",
            tmstats.matched, tmstats.skipped, tmstats.added, tmstats.immutable
        );
    }

    // Dropbox sync exclusions.
    if has_dropbox && !args.dont_sync_dropbox {
        let dbxpath = PathBuf::from(&dbx.path);

        if args.verbose {
            println!("\n- Excluding package dependencies from Dropbox Sync");
            println!("  - From {}", dbx.path);
        }

        let mut dbxstats = Stats::default();
        walk(
            WalkOptions {
                directory: &dbxpath,
                exclusions: &[],
                matchers: &matchers,
                attribute: XATTR_DROPBOX,
                root_path: starting_path,
                verbose: args.verbose,
                show_immutable: args.show_immutable,
                dry_run: args.dry_run,
            },
            &mut dbxstats,
        );

        if args.verbose {
            println!(
                "  % checked {}, skipped {}, added {}",
                dbxstats.matched, dbxstats.skipped, dbxstats.added,
            );
        }
    }

    Ok(())
}

fn walk(options: WalkOptions, stats: &mut Stats) {
    let mut it = WalkDir::new(options.directory).into_iter();
    loop {
        let entry = match it.next() {
            None => break,
            // Skip unreadable entries instead of aborting the whole walk.
            Some(Err(_)) => continue,
            Some(Ok(entry)) => entry,
        };

        if !entry.file_type().is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy();

        // Exclude some paths outright.
        if options.exclusions.contains(&name.as_ref()) {
            it.skip_current_dir();
            continue;
        }

        let Some(siblings) = options.matchers.get(name.as_ref()) else {
            continue;
        };
        let Some(parent) = entry.path().parent() else {
            continue;
        };

        for sibling_name in siblings {
            let sibling_path = parent.join(sibling_name);
            if !sibling_path.exists() {
                continue;
            }

            let path = entry.path().to_string_lossy().into_owned();
            if !is_writeable(&sibling_path) {
                if options.show_immutable {
                    println!(
                        "  ^ {} ",
                        sibling_path.to_string_lossy().replace(options.root_path, "~")
                    );
                }
                stats.immutable += 1;
                continue;
            }

            stats.matched += 1;
            if already_excluded(options.attribute, &path) {
                stats.skipped += 1;
            } else {
                stats.added += 1;
                if !options.dry_run {
                    exclude(options.attribute, &path);
                }
                // Always report additions on a dry run, otherwise only when verbose.
                if options.verbose || options.dry_run {
                    println!("  + {} ", path.replace(options.root_path, "~"));
                }
            }

            // The directory is handled and not traversed deeper; one matching
            // marker is enough, so stop checking further siblings.
            it.skip_current_dir();
            break;
        }
    }
}

pub fn already_excluded(key: &str, path: &str) -> bool {
    // An unreadable attribute (e.g. permissions) is treated as "not excluded".
    matches!(get(path, key), Ok(Some(_)))
}

pub fn is_writeable(path: impl AsRef<Path>) -> bool {
    File::options()
        .write(true)
        // Make sure we don't accidentally truncate the file.
        .truncate(false)
        .open(path.as_ref())
        .is_ok()
}

pub fn exclude(key: &str, path: &str) {
    let _ = set(path, key, &[1u8]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matchers_map_dirs_to_their_markers() {
        let matchers = build_matchers();
        assert_eq!(matchers.get("node_modules"), Some(&vec!["package.json"]));
        assert!(matchers["target"].contains(&"Cargo.toml"));
        assert!(!matchers.contains_key("not_a_build_dir"));
    }

    #[test]
    fn every_matcher_has_at_least_one_marker() {
        for (dir, markers) in build_matchers() {
            assert!(!dir.is_empty());
            assert!(!markers.is_empty(), "{dir} has no marker files");
        }
    }
}

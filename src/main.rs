use clap::Parser;
use std::path::Path;
use std::{collections::HashMap, process::Command, str};
use walkdir::WalkDir;
use xattr::{list, set};
extern crate ini;
use std::path::PathBuf;
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
}

#[derive(Debug)]
struct Stats {
    matched: u64,
    skipped: u64,
    added: u64,
}

const XATTR_DROPBOX: &str = "com.dropbox.ignored";
const XATTR_TIMEMACHINE: &str = "com.apple.metadata:com_apple_backup_excludeItem";

fn main() {
    let args = Args::parse();

    let matchers = HashMap::from([
        ("bower_components", "bower.json"), // oldschool js
        ("node_modules", "package.json"),   // node
        ("target", "Cargo.toml"),           // rust
        ("target", "pom.xml"),              // java/maven
        ("Pods", "Podfile"),                // cocoapods
    ]);

    let mut tm_exclude = vec!["Library", ".Trash"];

    let mut tmstats = Stats {
        added: 0,
        matched: 0,
        skipped: 0,
    };

    let mut dbxstats = Stats {
        added: 0,
        matched: 0,
        skipped: 0,
    };
    let homedir = dirs::home_dir().unwrap();

    let mut starting_path = homedir.to_str().unwrap();
    let specified_path = args.path.unwrap_or_default();
    if !specified_path.is_empty() {
        starting_path = &specified_path;
    }

    let maestral = dropbox::get_folder();

    let has_dropbox = maestral.is_some();
    let m = maestral.unwrap();
    let pp = dropbox::get_path_last_part(&m, '/');
    if has_dropbox && args.tm_skip_dropbox {
        tm_exclude.push(&pp);
    }

    if args.verbose {
        println!("- Preventing TimeMachine Backup ({})", starting_path);
    }

    // do time machine exclusions
    walk(
        &homedir,
        &tm_exclude,
        &matchers,
        XATTR_TIMEMACHINE,
        starting_path,
        &mut tmstats,
        args.verbose,
    );

    if args.verbose {
        println!(
            "  % checked {}, skipped {}, added {}",
            tmstats.matched, tmstats.skipped, tmstats.added,
        );
    }

    // lets to Dropbox
    let dbxpath = PathBuf::from(&m);

    if args.verbose {
        println!("\n- Preventing Dropbox Sync ({})", &m);
    }

    walk(
        &dbxpath,
        &[],
        &matchers,
        XATTR_DROPBOX,
        starting_path,
        &mut dbxstats,
        args.verbose,
    );

    if args.verbose {
        println!(
            "  % checked {}, skipped {}, added {}",
            dbxstats.matched, dbxstats.skipped, dbxstats.added,
        );
    }
}

fn walk(
    root: &PathBuf,
    exclusions: &[&str],
    matchers: &HashMap<&str, &str>,
    key: &str,
    replace: &str,
    stats: &mut Stats,
    verbose: bool,
) {
    let mut it = WalkDir::new(root).into_iter();
    loop {
        let entry = match it.next() {
            None => break,
            Some(Err(err)) => panic!("ERROR: {}", err),
            Some(Ok(entry)) => entry,
        };

        if entry.file_type().is_dir() {
            let path = String::from(entry.file_name().to_string_lossy());

            // Exclude some paths
            if exclusions.contains(&path.as_str()) {
                it.skip_current_dir();
            }

            if matchers.contains_key(&path.as_str()) {
                let parent_path = entry.path().parent().unwrap().to_str();
                let sibling_name = matchers.get(&path.as_str());
                let sibling = [parent_path.unwrap(), sibling_name.unwrap()].join("/");

                if Path::new(sibling.as_str()).exists() {
                    stats.matched += 1;
                    let path = String::from(entry.path().to_string_lossy());

                    if !already_excluded(key, &path) {
                        stats.added += 1;
                        // Add the time machine exclusion, show the excluded dir and size
                        exclude(key, &path);
                        // Add the time machine exclusion, show the excluded dir and size
                        let size = size_of(&path);
                        if verbose {
                            println!("+ {} ({})", path.replace(replace, "~"), size);
                        }
                    } else {
                        stats.skipped += 1
                    }
                    // no need to traverse any deeper
                    it.skip_current_dir();
                }
            }
        }
    }
}

pub fn already_excluded(key: &str, path: &str) -> bool {
    let mut xattrs = list(path).unwrap().peekable();
    if xattrs.peek().is_none() {
        return false;
    }
    for attr in xattrs {
        if attr == key {
            return true;
        }
    }
    false
}

pub fn exclude(key: &str, path: &str) {
    let value = vec![1; 1];
    let _ = set(path, key, &value);
}

pub fn size_of(path: &str) -> String {
    let output = Command::new("du").arg("-hs").arg(path).output().unwrap();
    let chunks: Vec<&str> = str::from_utf8(&output.stdout[..])
        .unwrap()
        .split('\t')
        .collect();
    return chunks[0].trim().to_string();
}

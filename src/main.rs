use clap::Parser;
use dropbox::DropBox;
use std::fs::File;
use std::path::Path;
use std::{collections::HashMap, str};
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
    #[arg(long)]
    show_immutable: bool,
}

#[derive(Debug)]
struct Stats {
    matched: u64,
    skipped: u64,
    added: u64,
    immutable: u64,
}

struct WalkOptions<'a> {
    pub directory: &'a PathBuf,
    pub exclusions: &'a [&'a str],
    pub matchers: &'a HashMap<&'a str, Vec<&'a str>>,
    pub attribute: &'a str,
    pub root_path: &'a str,
    pub verbose: bool,
    pub show_immutable: bool,
}
const XATTR_DROPBOX: &str = "com.dropbox.ignored";
const XATTR_TIMEMACHINE: &str = "com.apple.metadata:com_apple_backup_excludeItem";

fn main() {
    let args = Args::parse();

    let mut matchers = HashMap::new();
    matchers.insert("bower_components", vec!["bower.json"]);
    matchers.insert("node_modules", vec!["package.json"]);
    matchers.insert("target", vec!["Cargo.toml", "pox.xml"]);
    matchers.insert("Pods", vec!["Podfile"]);
    matchers.insert("vendor", vec!["go.mod"]);

    // paths we exclude
    let mut tm_exclude = vec!["Library", ".Trash"];

    let mut tmstats = Stats {
        added: 0,
        matched: 0,
        skipped: 0,
        immutable: 0,
    };

    let mut dbxstats = Stats {
        added: 0,
        matched: 0,
        skipped: 0,
        immutable: 0,
    };
    let homedir = dirs::home_dir().unwrap();

    let mut starting_path = homedir.to_str().unwrap();
    let specified_path = args.path.unwrap_or_default();
    if !specified_path.is_empty() {
        starting_path = &specified_path;
    }

    let mut dbx = DropBox::new();
    let dbxname = &dbx.name();
    let dbxpath = &dbx.folder();
    let has_dropbox = !dbx.path.is_empty();

    // if dbx is already excluded from time machine, no need to traverse
    if has_dropbox && args.tm_skip_dropbox {
        tm_exclude.push(dbxname);
    }

    if args.verbose {
        println!("- Excluding package dependencies from Time Machine");
        println!("  - From {}", starting_path);
    }

    let tmotions = WalkOptions {
        directory: &homedir,
        exclusions: &tm_exclude,
        matchers: &matchers,
        attribute: XATTR_TIMEMACHINE,
        root_path: starting_path,
        verbose: args.verbose,
        show_immutable: args.show_immutable,
    };

    // do time machine exclusions
    walk(tmotions, &mut tmstats);

    if args.verbose {
        println!(
            "  % checked {}, skipped {}, added {}, immutable: {}",
            tmstats.matched, tmstats.skipped, tmstats.added, tmstats.immutable
        );
    }

    // lets to Dropbox
    let dbxpath = PathBuf::from(&dbxpath);

    if args.verbose {
        println!("\n- Excluding package dependencies from Dropbox Sync");
        println!("  - From {}", &dbx.path);
    }

    let dbxoptions = WalkOptions {
        directory: &dbxpath,
        exclusions: &[],
        matchers: &matchers,
        attribute: XATTR_DROPBOX,
        root_path: starting_path,
        verbose: args.verbose,
        show_immutable: args.show_immutable,
    };

    walk(dbxoptions, &mut dbxstats);

    if args.verbose {
        println!(
            "  % checked {}, skipped {}, added {}",
            dbxstats.matched, dbxstats.skipped, dbxstats.added,
        );
    }
}

fn walk(options: WalkOptions, stats: &mut Stats) {
    let mut it = WalkDir::new(options.directory).into_iter();
    loop {
        let entry = match it.next() {
            None => break,
            Some(Err(err)) => panic!("ERROR: {}", err),
            Some(Ok(entry)) => entry,
        };

        if entry.file_type().is_dir() {
            let path = String::from(entry.file_name().to_string_lossy());

            // Exclude some paths
            if options.exclusions.contains(&path.as_str()) {
                it.skip_current_dir();
            }

            if options.matchers.contains_key(&path.as_str()) {
                let parent_path = entry.path().parent().unwrap().to_str();
                let siblings = options.matchers.get(&path.as_str());
                for sibling_name in siblings.unwrap().iter() {
                    let sibling = [parent_path.unwrap(), sibling_name].join("/");
                    let sibling_path = Path::new(sibling.as_str());
                    if sibling_path.exists() {
                        let path = String::from(entry.path().to_string_lossy());
                        if !is_writeable(sibling_path) {
                            if options.show_immutable {
                                println!(
                                    "  ^ {} ",
                                    sibling_path
                                        .to_string_lossy()
                                        .replace(options.root_path, "~")
                                );
                            }
                            stats.immutable += 1;
                            continue;
                        }
                        stats.matched += 1;

                        if !already_excluded(options.attribute, &path) {
                            stats.added += 1;
                            // Add  exclusion, show the excluded dir and size
                            exclude(options.attribute, &path);
                            if options.verbose {
                                println!("  + {} ", path.replace(options.root_path, "~"));
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

pub fn is_writeable(path: impl AsRef<Path>) -> bool {
    File::options()
        .write(true)
        // Make sure we don't accidentally truncate the file.
        .truncate(false)
        .open(path.as_ref())
        .is_ok()
}

pub fn exclude(key: &str, path: &str) {
    let value = vec![1; 1];
    let _ = set(path, key, &value);
}

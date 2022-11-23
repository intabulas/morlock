use clap::Parser;
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;
extern crate ini;
use std::path::PathBuf;
mod dropbox;
mod utils;

use dropbox::*;
use utils::*;

// use std::{
//     fs::File,
//     io::{prelude::*, BufReader},
// };

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    verbose: bool,
    #[arg(long)]
    skip_dropbox: bool,
    #[arg(long)]
    dont_sync_dropbox: bool,
}

#[derive(Debug)]
struct Stats {
    matched: u64,
    skipped: u64,
    added: u64,
}

fn main() {
    let args = Args::parse();

    let matchers = HashMap::from([
        ("bower_components", "bower.json"), // oldschool js
        ("node_modules", "package.json"),   // node
        ("target", "Cargo.toml"),           // rust
        ("target", "pom.xml"),              // java/maven
        ("Pods", "Podfile"),                // cocoapods
    ]);

    let mut exclude = vec!["Library", ".Trash"];

    let mut stats = Stats {
        added: 0,
        matched: 0,
        skipped: 0,
    };

    let homedir = dirs::home_dir().unwrap();
    let hd = homedir.to_str().unwrap();

    let maestral = determine_dropbox_folder();

    let has_dropbox = maestral.is_some();
    let m = maestral.unwrap().clone();
    let pp = get_path_last_part(&m, '/');
    if has_dropbox && args.skip_dropbox {
        exclude.push(&pp);
    }

    println!("hunting from {}", homedir.display());
    if has_dropbox {
        exclude.push("Dropbox");
    }

    // do time machine exclusions
    walk(
        &homedir,
        &exclude,
        &matchers,
        is_already_excluded,
        exclude_path,
        hd,
        &mut stats,
        args.verbose,
    );

    let dbxpath = PathBuf::from(&m);

    // print!("\n\n=============\n\n");

    walk(
        &dbxpath,
        &vec![],
        &matchers,
        is_already_ignored,
        dont_sync_path,
        hd,
        &mut stats,
        args.verbose,
    );

    println!(
        "@ matched {}, skipped {}, added {}",
        stats.matched, stats.skipped, stats.added,
    );
}

fn walk(
    root: &PathBuf,
    exclusions: &Vec<&str>,
    matchers: &HashMap<&str, &str>,
    already_excluded: fn(&str) -> bool,
    exclude: fn(&str),
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
                if verbose {
                    println!("^ {}", path.replace(replace, "~"));
                }
                it.skip_current_dir();
            }

            if matchers.contains_key(&path.as_str()) {
                let parent_path = entry.path().parent().unwrap().to_str();
                let sibling_name = matchers.get(&path.as_str());
                let sibling = format!("{}/{}", parent_path.unwrap(), sibling_name.unwrap());

                if Path::new(sibling.as_str()).exists() {
                    stats.matched += 1;
                    let path = String::from(entry.path().to_string_lossy());

                    if verbose {
                        println!("! {}", path.replace(replace, "~"));
                    }

                    if !already_excluded(&path) {
                        stats.added += 1;
                        // Add the time machine exclusion, show the excluded dir and size
                        exclude(&path);
                        // Add the time machine exclusion, show the excluded dir and size
                        let size = size_of_path(&path);
                        println!("+ {} ({})", path.replace(replace, "~"), size);
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

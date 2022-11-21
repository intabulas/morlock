use clap::Parser;
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;
extern crate ini;
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

    let mut matched: u64 = 0;
    let mut skipped: u64 = 0;
    let mut added: u64 = 0;

    let homedir = dirs::home_dir().unwrap();
    let hd = homedir.to_str().unwrap();

    let maestral = determine_dropbox_folder();

    let has_dropbox = maestral.is_some();

    let pp = get_path_last_part(maestral.unwrap(), '/');
    if has_dropbox && args.skip_dropbox {
        exclude.push(&pp);
    }

    println!("hunting from {}", homedir.display());
    if has_dropbox {
        exclude.push("Dropbox");
    }

    let mut it = WalkDir::new(&homedir).into_iter();
    loop {
        let entry = match it.next() {
            None => break,
            Some(Err(err)) => panic!("ERROR: {}", err),
            Some(Ok(entry)) => entry,
        };

        if entry.file_type().is_dir() {
            let path = String::from(entry.file_name().to_string_lossy());

            // Exclude some paths
            if exclude.contains(&path.as_str()) {
                if args.verbose {
                    println!("^ {}", path.replace(hd, "~"));
                }
                it.skip_current_dir();
            }

            if matchers.contains_key(&path.as_str()) {
                let parent_path = entry.path().parent().unwrap().to_str();
                let sibling_name = matchers.get(&path.as_str());
                let sibling = format!("{}/{}", parent_path.unwrap(), sibling_name.unwrap());

                if Path::new(sibling.as_str()).exists() {
                    matched += 1;
                    let path = String::from(entry.path().to_string_lossy());

                    // if args.verbose {
                    //     println!("! {}", path.replace(hd, "~"));
                    // }

                    if !is_already_excluded(&path) {
                        added += 1;
                        // Add the time machine exclusion, show the excluded dir and size
                        exlcude_path(&path);
                        // Add the time machine exclusion, show the excluded dir and size
                        let size = size_of_path(&path);
                        println!("+ {} ({})", path.replace(hd, "~"), size);
                    } else {
                        skipped += 1
                    }
                    // no need to traverse any deeper
                    it.skip_current_dir();
                }
            }
        }
    }

    println!(
        "@ matched {}, skipped {}, added {}",
        matched, skipped, added,
    );
}

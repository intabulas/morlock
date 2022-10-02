use clap::Parser;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::str;
use walkdir::WalkDir;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    verbose: bool,
}

// see if the path is already excluded from tm
fn is_already_excluded(path: &str) -> bool {
    let isexcluded = Command::new("tmutil")
        .arg("isexcluded")
        .arg(&path)
        .output()
        .unwrap();

    str::from_utf8(&isexcluded.stdout[..])
        .unwrap()
        .starts_with("[Excluded]")
}

// exclude a path from tm
fn exlcude_path(path: &str) {
    let _ = Command::new("tmutil")
        .arg("addexclusion")
        .arg(&path)
        .output();
}

// get asize of path in human readable for showing in stats
fn size_of_path(path: &str) -> String {
    let output = Command::new("du").arg("-hs").arg(&path).output().unwrap();
    let chunks: Vec<&str> = str::from_utf8(&output.stdout[..])
        .unwrap()
        .split("\t")
        .collect();
    return chunks[0].trim().to_string();
}

fn main() {
    let args = Args::parse();

    let matchers = HashMap::from([
        ("bower_components", "bower.json"), // oldschool js
        ("node_modules", "package.json"),   // node
        ("target", "Cargo.toml"),           // rust
    ]);

    let exclude = vec!["Library", ".Trash"];

    let mut matched: u64 = 0;
    let mut skipped: u64 = 0;
    let mut added: u64 = 0;

    let mut it = WalkDir::new(dirs::home_dir().unwrap()).into_iter();
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
                it.skip_current_dir();
            }
            if matchers.contains_key(&path.as_str()) {
                let parent_path = entry.path().parent().unwrap().to_str();
                let sibling_name = matchers.get(&path.as_str());
                let sibling = format!("{}/{}", parent_path.unwrap(), sibling_name.unwrap());

                if Path::new(sibling.as_str()).exists() {
                    matched += 1;
                    let path = String::from(entry.path().to_string_lossy());

                    if args.verbose {
                        println!("{} ", path);
                    }

                    if !is_already_excluded(&path) {
                        added += 1;
                        // Add the time machine exclusion, show the excluded dir and size
                        exlcude_path(&path);
                        // Add the time machine exclusion, show the excluded dir and size
                        let size = size_of_path(&path);
                        println!("{} ({})", path, size);
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
        "matched: {}, skipped: {}, added: {}",
        matched, skipped, added,
    );
}

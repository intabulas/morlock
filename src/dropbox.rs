use ini::Ini;
extern crate base64;
use base64::decode;
use std::path::Path;
use std::str;
use xattr::{list, set};

use std::{
    fs::File,
    io::{prelude::*, BufReader},
};

const PATH_MAESTRAL: &str = "/Library/Application Support/maestral/maestral.ini";
const PATH_DROPBOX: &str = "/.dropbox/host.db";
const DROPBOX_ATTR: &str = "com.dropbox.ignored";

pub fn determine_dropbox_folder() -> Option<String> {
    let homedir = dirs::home_dir().unwrap().display().to_string();

    let maestral_path = format!("{}{}", homedir, PATH_MAESTRAL);

    let dbx = format!("{}{}", homedir, PATH_DROPBOX);

    if Path::new(&dbx).exists() {
        let file = File::open(dbx).expect("no such file");
        let buf = BufReader::new(file);
        let lines: Vec<String> = buf
            .lines()
            .map(|l| l.expect("Could not parse line"))
            .collect();

        let bytes = decode(lines[1].as_bytes()).unwrap();
        return Some(String::from_utf8(bytes.clone()).unwrap());
    } else if Path::new(&maestral_path).exists() {
        let conf = Ini::load_from_file(maestral_path).unwrap();
        let section = conf.section(Some("sync")).unwrap();
        let path = section.get("path").unwrap();

        return Some(path.to_string());
    } else {
        return None;
    }

    // Are they using maestral? they should be
}

pub fn get_path_last_part(path: &str, sep: char) -> String {
    let pieces = path.split(sep);
    match pieces.last() {
        Some(p) => p.into(),
        None => path.into(),
    }
}

pub fn is_already_ignored(path: &str) -> bool {
    let mut xattrs = list(path).unwrap().peekable();
    if xattrs.peek().is_none() {
        return false;
    }
    for attr in xattrs {
        if attr == DROPBOX_ATTR {
            return true;
        }
    }
    false
}

// exclude a path from tm
pub fn dont_sync_path(path: &str) {
    let value = vec![1; 1];
    let _ = set(path, DROPBOX_ATTR, &value);
}

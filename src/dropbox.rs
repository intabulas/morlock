use ini::Ini;
extern crate base64;
use base64::decode;
use std::path::Path;
use std::str;
// add code here

use std::{
    fs::File,
    io::{prelude::*, BufReader},
};

const PATH_MAESTRAL: &str = "/Library/Application Support/maestral/maestral.ini";
const PATH_DROPBOX: &str = "/.dropbox/host.db";

pub fn get_folder() -> Option<String> {
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
        Some(String::from_utf8(bytes).unwrap())
    } else if Path::new(&maestral_path).exists() {
        let conf = Ini::load_from_file(maestral_path).unwrap();
        let section = conf.section(Some("sync")).unwrap();
        let path = section.get("path").unwrap();

        Some(path.to_string())
    } else {
        None
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

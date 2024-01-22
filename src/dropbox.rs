use ini::Ini;
extern crate base64;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use std::path::Path;
use std::str;
// add code here

use std::{
    fs::File,
    io::{prelude::*, BufReader},
};

const PATH_MAESTRAL: &str = "/Library/Application Support/maestral/maestral.ini";
const PATH_DROPBOX: &str = "/.dropbox/host.db";

pub struct DropBox {
    pub path: String,
}

impl DropBox {
    pub fn new() -> Self {
        Self {
            path: "".to_string(),
        }
    }

    pub fn folder(&mut self) -> String {
        if self.path.is_empty() {
            self.path = self.get_folder();
        }
        self.path.clone()
    }

    fn get_folder(&self) -> String {
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

            let bytes = BASE64.decode(lines[1].as_bytes()).unwrap();
            String::from_utf8(bytes).unwrap()
        } else if Path::new(&maestral_path).exists() {
            let conf = Ini::load_from_file(maestral_path).unwrap();
            let section = conf.section(Some("sync")).unwrap();
            let path = section.get("path").unwrap();

            path.to_string()
        } else {
            "".to_string()
        }

        // Are they using maestral? they should be
    }

    pub fn name(&self) -> String {
        let pieces = self.path.split('/');
        pieces.last().unwrap().to_string()
    }
}

use std::process::Command;
use std::str;

pub fn is_already_excluded(path: &str) -> bool {
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
pub fn exlcude_path(path: &str) {
    let _ = Command::new("tmutil")
        .arg("addexclusion")
        .arg(&path)
        .output();
}

pub fn size_of_path(path: &str) -> String {
    let output = Command::new("du").arg("-hs").arg(&path).output().unwrap();
    let chunks: Vec<&str> = str::from_utf8(&output.stdout[..])
        .unwrap()
        .split("\t")
        .collect();
    return chunks[0].trim().to_string();
}

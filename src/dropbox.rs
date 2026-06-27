use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ini::Ini;
use std::fs;

/// The official Dropbox client stores a base64-encoded path in host.db.
const DROPBOX_HOST_DB: &str = ".dropbox/host.db";
/// Maestral stores its sync folder in an INI file under the home directory.
const MAESTRAL_INI: &str = "Library/Application Support/maestral/maestral.ini";

#[derive(Default)]
pub struct DropBox {
    pub path: String,
}

impl DropBox {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve and cache the Dropbox folder. Returns an empty string when no
    /// Dropbox or Maestral installation is found.
    pub fn folder(&mut self) -> Result<&str> {
        if self.path.is_empty() {
            self.path = Self::resolve_folder()?;
        }
        Ok(&self.path)
    }

    fn resolve_folder() -> Result<String> {
        let home = dirs::home_dir().context("could not determine home directory")?;
        let host_db = home.join(DROPBOX_HOST_DB);
        let maestral = home.join(MAESTRAL_INI);

        if host_db.exists() {
            let contents = fs::read_to_string(&host_db)
                .with_context(|| format!("reading {}", host_db.display()))?;
            parse_host_db(&contents)
        } else if maestral.exists() {
            let contents = fs::read_to_string(&maestral)
                .with_context(|| format!("reading {}", maestral.display()))?;
            parse_maestral_ini(&contents)
        } else {
            Ok(String::new())
        }
    }

    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or_default()
    }
}

/// Decode the Dropbox folder path from the contents of `.dropbox/host.db`,
/// whose second line is the base64-encoded UTF-8 path.
fn parse_host_db(contents: &str) -> Result<String> {
    let encoded = contents
        .lines()
        .nth(1)
        .context("host.db did not contain a path on its second line")?;
    let bytes = BASE64
        .decode(encoded.as_bytes())
        .context("decoding base64 path from host.db")?;
    String::from_utf8(bytes).context("Dropbox path in host.db was not valid UTF-8")
}

/// Read the sync folder path from the contents of Maestral's `maestral.ini`.
fn parse_maestral_ini(contents: &str) -> Result<String> {
    let conf = Ini::load_from_str(contents).context("parsing maestral.ini")?;
    let path = conf
        .section(Some("sync"))
        .and_then(|s| s.get("path"))
        .context("maestral.ini is missing a [sync] path entry")?;
    Ok(path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_db_decodes_base64_second_line() {
        let encoded = BASE64.encode("/Users/me/Dropbox");
        let contents = format!("first-line-is-ignored\n{encoded}\n");
        assert_eq!(parse_host_db(&contents).unwrap(), "/Users/me/Dropbox");
    }

    #[test]
    fn host_db_missing_second_line_errors() {
        assert!(parse_host_db("only-one-line").is_err());
    }

    #[test]
    fn host_db_invalid_base64_errors() {
        assert!(parse_host_db("line1\nthis is not base64!!").is_err());
    }

    #[test]
    fn maestral_reads_sync_path() {
        let ini = "[sync]\npath = /Users/me/Dropbox\n";
        assert_eq!(parse_maestral_ini(ini).unwrap(), "/Users/me/Dropbox");
    }

    #[test]
    fn maestral_missing_path_errors() {
        let ini = "[main]\nfoo = bar\n";
        assert!(parse_maestral_ini(ini).is_err());
    }

    #[test]
    fn name_returns_last_path_segment() {
        let dbx = DropBox {
            path: "/Users/me/Dropbox".to_string(),
        };
        assert_eq!(dbx.name(), "Dropbox");
    }

    #[test]
    fn name_handles_no_slash_and_empty_path() {
        assert_eq!(DropBox { path: "Dropbox".into() }.name(), "Dropbox");
        assert_eq!(DropBox::new().name(), "");
    }
}

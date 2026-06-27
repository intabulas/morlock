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
            let encoded = contents
                .lines()
                .nth(1)
                .context("host.db did not contain a path on its second line")?;
            let bytes = BASE64
                .decode(encoded.as_bytes())
                .context("decoding base64 path from host.db")?;
            String::from_utf8(bytes).context("Dropbox path in host.db was not valid UTF-8")
        } else if maestral.exists() {
            let conf = Ini::load_from_file(&maestral)
                .with_context(|| format!("loading {}", maestral.display()))?;
            let path = conf
                .section(Some("sync"))
                .and_then(|s| s.get("path"))
                .context("maestral.ini is missing a [sync] path entry")?;
            Ok(path.to_string())
        } else {
            Ok(String::new())
        }
    }

    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or_default()
    }
}

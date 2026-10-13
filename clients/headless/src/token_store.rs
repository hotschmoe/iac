// Auth tokens, remembered per server and player name in a small JSON file
// under the user's config dir (e.g. ~/.config/iac/tokens.json):
//
//   { "127.0.0.1:7777": { "admiral": "<64 hex chars>" } }
//
// Names are keyed lowercase, matching the server's case-insensitive names.
// The token is the account's only secret, so the file is created 0600.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

type Tokens = BTreeMap<String, BTreeMap<String, String>>;

pub fn server_key(host: &str, port: u16) -> String {
    format!("{host}:{port}")
}

pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("iac").join("tokens.json"))
}

pub fn load(server: &str, name: &str) -> Option<String> {
    load_from(&default_path()?, server, name)
}

/// Save the token and return the file it went to.
pub fn save(server: &str, name: &str, token: &str) -> io::Result<PathBuf> {
    let path = default_path().ok_or_else(|| io::Error::other("no user config directory"))?;
    save_to(&path, server, name, token)?;
    Ok(path)
}

fn read_all(path: &Path) -> io::Result<Tokens> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {e}", path.display()))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Tokens::new()),
        Err(e) => Err(e),
    }
}

fn load_from(path: &Path, server: &str, name: &str) -> Option<String> {
    read_all(path).ok()?.get(server)?.get(&name.to_lowercase()).cloned()
}

fn save_to(path: &Path, server: &str, name: &str, token: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Clients logging in at the same moment would otherwise each write back
    // their own read of the file and drop the other's token.
    let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(path.with_extension("json.lock"))?;
    lock.lock()?;

    // A file we cannot parse is left alone rather than overwritten.
    let mut all = read_all(path)?;
    all.entry(server.to_string()).or_default().insert(name.to_lowercase(), token.to_string());

    let tmp = path.with_extension("json.tmp");
    write_private(&tmp, serde_json::to_string_pretty(&all)?.as_bytes())?;
    std::fs::rename(&tmp, path)
}

#[cfg(unix)]
fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    f.write_all(data)
}

#[cfg(not(unix))]
fn write_private(path: &Path, data: &[u8]) -> io::Result<()> {
    std::fs::write(path, data)
}

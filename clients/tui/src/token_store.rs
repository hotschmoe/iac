// Auth tokens, remembered per server and player name in a small JSON file
// under the user's config dir (e.g. ~/.config/iac/tokens.json):
//
//   { "127.0.0.1:7777": { "Admiral": "<64 hex chars>" } }
//
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
    read_all(path).ok()?.get(server)?.get(name).cloned()
}

fn save_to(path: &Path, server: &str, name: &str, token: &str) -> io::Result<()> {
    // A file we cannot parse is left alone rather than overwritten.
    let mut all = read_all(path)?;
    all.entry(server.to_string()).or_default().insert(name.to_string(), token.to_string());

    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("iac_tokens_{name}_{}", std::process::id()));
        dir.join("iac").join("tokens.json")
    }

    #[test]
    fn tokens_are_kept_per_server_and_name() {
        let path = temp_file("keyed");
        assert_eq!(load_from(&path, "a:1", "Ann"), None);

        save_to(&path, "a:1", "Ann", "tok-a1-ann").unwrap();
        save_to(&path, "a:1", "Bob", "tok-a1-bob").unwrap();
        save_to(&path, "b:2", "Ann", "tok-b2-ann").unwrap();

        assert_eq!(load_from(&path, "a:1", "Ann").as_deref(), Some("tok-a1-ann"));
        assert_eq!(load_from(&path, "a:1", "Bob").as_deref(), Some("tok-a1-bob"));
        assert_eq!(load_from(&path, "b:2", "Ann").as_deref(), Some("tok-b2-ann"));
        assert_eq!(load_from(&path, "b:2", "Bob"), None);

        save_to(&path, "a:1", "Ann", "rotated").unwrap();
        assert_eq!(load_from(&path, "a:1", "Ann").as_deref(), Some("rotated"));
    }

    #[cfg(unix)]
    #[test]
    fn token_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_file("mode");
        save_to(&path, "a:1", "Ann", "secret").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn an_unreadable_file_is_not_clobbered() {
        let path = temp_file("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();
        assert!(save_to(&path, "a:1", "Ann", "t").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
        assert_eq!(load_from(&path, "a:1", "Ann"), None);
    }
}

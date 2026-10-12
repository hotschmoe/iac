// Player authentication primitives: name rules and bearer token checks. Token
// issuing needs OS entropy, so the host supplies it (see `GameEngine::authenticate`).
// Design and rationale: docs/auth_spec.md ("Implemented subset").

use iac_shared::protocol::{is_bot_name, BOT_NAME_PREFIX};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

pub type TokenHash = [u8; 32];

pub const NAME_MIN_LEN: usize = 3;
pub const NAME_MAX_LEN: usize = 24;

const RESERVED_NAMES: [&str; 10] = [
    "admin", "administrator", "server", "system", "moderator", "mod", "npc", "mlm", "gm", "gamemaster",
];

/// SHA-256 of the token text. Tokens are 256 random bits, so no salt or
/// slow KDF is needed.
pub fn hash_token(token: &str) -> TokenHash {
    Sha256::digest(token.as_bytes()).into()
}

/// A token as an account is handed it: 256 bits the host drew, hex encoded
/// (64 characters).
pub fn token_text(bits: &[u8; 32]) -> String {
    bits.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn token_matches(stored: &TokenHash, presented: &str) -> bool {
    stored.ct_eq(&hash_token(presented)).into()
}

/// Rules for newly registered names; existing accounts keep whatever name
/// they already have. Returns a human-readable reason on rejection.
pub fn validate_name(name: &str) -> Result<(), String> {
    well_formed(name)?;
    if RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(name)) {
        return Err("that name is reserved".to_string());
    }
    if is_bot_name(name) {
        return Err(format!("names starting with '{BOT_NAME_PREFIX}' are reserved for built-in bots"));
    }
    Ok(())
}

/// The rules a built-in bot's name must meet: the shape of any name, and the
/// prefix no other account may carry.
pub fn validate_bot_name(name: &str) -> Result<(), String> {
    well_formed(name)?;
    if !is_bot_name(name) {
        return Err(format!("a bot's name must start with '{BOT_NAME_PREFIX}'"));
    }
    Ok(())
}

fn well_formed(name: &str) -> Result<(), String> {
    let len = name.chars().count();
    if !(NAME_MIN_LEN..=NAME_MAX_LEN).contains(&len) {
        return Err(format!("must be {NAME_MIN_LEN}-{NAME_MAX_LEN} characters long (got {len})"));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err("may only contain letters, digits, '_' and '-'".to_string());
    }
    if name.starts_with(['-', '_']) || name.ends_with(['-', '_']) {
        return Err("must not start or end with '-' or '_'".to_string());
    }
    Ok(())
}

// Player authentication primitives: name rules and bearer token checks. Token
// issuing needs OS entropy, so the host supplies it (see `GameEngine::authenticate`).
// Design and rationale: docs/auth_spec.md ("Implemented subset").

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

pub fn token_matches(stored: &TokenHash, presented: &str) -> bool {
    stored.ct_eq(&hash_token(presented)).into()
}

/// Rules for newly registered names; existing accounts keep whatever name
/// they already have. Returns a human-readable reason on rejection.
pub fn validate_name(name: &str) -> Result<(), String> {
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
    if RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(name)) {
        return Err("that name is reserved".to_string());
    }
    Ok(())
}

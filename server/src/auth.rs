// Token issuing, the one part of authentication that needs OS entropy and so
// stays out of iac-sim. Hashing, checking and the token's text live in
// `iac_sim::auth`.

use iac_sim::auth::token_text;
use rand::RngCore;

/// 256 random bits from the OS-seeded CSPRNG.
pub fn generate_token() -> String {
    let mut bits = [0u8; 32];
    rand::rng().fill_bytes(&mut bits);
    token_text(&bits)
}

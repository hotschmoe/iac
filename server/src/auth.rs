// Token issuing, the one part of authentication that needs OS entropy and so
// stays out of iac-sim. Hashing and checking live in `iac_sim::auth`.

use rand::RngCore;

const TOKEN_BYTES: usize = 32;

/// 256 random bits from the OS-seeded CSPRNG, hex encoded (64 characters).
pub fn generate_token() -> String {
    let mut bytes = [0u8; TOKEN_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_64_hex_chars_and_unique() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}

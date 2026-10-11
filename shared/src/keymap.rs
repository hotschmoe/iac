// Hash maps and sets keyed by what the engine numbers itself: sector keys,
// entity ids, small tuples of them.
//
// The standard hasher resists keys chosen by an attacker, and for one
// integer that costs more than the lookup it serves; path searches and the
// chart upkeep do millions of these a minute. This one multiplies and
// folds. Iteration order is as arbitrary as with the standard hasher:
// nothing may depend on it.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

const MULTIPLIER: u64 = 0x9E37_79B9_7F4A_7C15;

#[derive(Debug, Default, Clone, Copy)]
pub struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, n: u8) {
        self.write_u64(u64::from(n));
    }

    fn write_u16(&mut self, n: u16) {
        self.write_u64(u64::from(n));
    }

    fn write_u32(&mut self, n: u32) {
        self.write_u64(u64::from(n));
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(MULTIPLIER);
    }

    fn write_usize(&mut self, n: usize) {
        self.write_u64(n as u64);
    }

    // The product's high bits depend on every bit of the key and its low
    // bits only on the key's low bits; a table indexes by the low ones.
    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 32)
    }
}

pub type KeyMap<K, V> = HashMap<K, V, BuildHasherDefault<KeyHasher>>;
pub type KeySet<K> = HashSet<K, BuildHasherDefault<KeyHasher>>;

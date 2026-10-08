// Axial hex coordinates with cube distance calculations.
// Flat-top orientation. Stored as (q, r), cube s = -q - r derived.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Hex coordinate in axial form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Hex {
    pub q: i16,
    pub r: i16,
}

impl Hex {
    pub const ORIGIN: Self = Hex { q: 0, r: 0 };
    /// Create a new hex from axial coordinates.
    #[inline]
    pub const fn new(q: i16, r: i16) -> Self {
        Hex { q, r }
    }

    /// Cube coordinate s, derived from q and r.
    #[inline]
    pub fn s(&self) -> i16 {
        -self.q - self.r
    }

    /// Cube distance between two hexes.
    pub fn distance(a: &Hex, b: &Hex) -> u16 {
        let dq = (a.q as i32) - (b.q as i32);
        let dr = (a.r as i32) - (b.r as i32);
        let ds = (a.s() as i32) - (b.s() as i32);
        dq.abs().max(dr.abs()).max(ds.abs()) as u16
    }

    /// Distance from the origin.
    pub fn dist_from_origin(&self) -> u16 {
        Self::distance(self, &Self::ORIGIN)
    }

    /// Add another hex vector.
    // Mirrors the Zig API; intentionally not the std::ops::Add trait.
    #[allow(clippy::should_implement_trait)]
    pub fn add(self, other: Hex) -> Hex {
        Hex {
            q: self.q + other.q,
            r: self.r + other.r,
        }
    }

    /// Subtract another hex.
    // Mirrors the Zig API; intentionally not the std::ops::Sub trait.
    #[allow(clippy::should_implement_trait)]
    pub fn sub(self, other: Hex) -> Hex {
        Hex {
            q: self.q - other.q,
            r: self.r - other.r,
        }
    }

    /// Get neighbor in the given direction.
    pub fn neighbor(self, dir: HexDirection) -> Hex {
        self.add(dir.to_vec())
    }

    /// Get all 6 neighbor coordinates.
    pub fn neighbors(self) -> [Hex; 6] {
        HexDirection::ALL.map(|dir| self.neighbor(dir))
    }

    /// Pack into a u32 for use as hash map key.
    pub fn to_key(self) -> u32 {
        (self.q as u32 as u16 as u32) | ((self.r as u32 as u16 as u32) << 16)
    }

    /// Unpack from a u32 key.
    pub fn from_key(key: u32) -> Hex {
        Hex {
            q: (key as u16) as i16,
            r: ((key >> 16) as u16) as i16,
        }
    }
}

impl fmt::Display for Hex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{},{}]", self.q, self.r)
    }
}

/// The 6 hex directions (flat-top orientation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum HexDirection {
    East = 0,
    NorthEast = 1,
    NorthWest = 2,
    West = 3,
    SouthWest = 4,
    SouthEast = 5,
}

impl HexDirection {
    pub const ALL: [HexDirection; 6] = [
        Self::East,
        Self::NorthEast,
        Self::NorthWest,
        Self::West,
        Self::SouthWest,
        Self::SouthEast,
    ];

    /// Direction vector in axial coordinates.
    pub fn to_vec(self) -> Hex {
        match self {
            Self::East => Hex { q: 1, r: 0 },
            Self::NorthEast => Hex { q: 1, r: -1 },
            Self::NorthWest => Hex { q: 0, r: -1 },
            Self::West => Hex { q: -1, r: 0 },
            Self::SouthWest => Hex { q: -1, r: 1 },
            Self::SouthEast => Hex { q: 0, r: 1 },
        }
    }

    /// Opposite direction.
    pub fn opposite(self) -> HexDirection {
        let idx = (self as u32 + 3) % 6;
        unsafe { std::mem::transmute(idx) }
    }

    /// Short label for display.
    pub fn label(self) -> &'static str {
        match self {
            Self::East => "E",
            Self::NorthEast => "NE",
            Self::NorthWest => "NW",
            Self::West => "W",
            Self::SouthWest => "SW",
            Self::SouthEast => "SE",
        }
    }
}

/// Iterate over all hexes in a ring at a given radius from center.
pub fn hex_ring(center: Hex, radius: u16) -> HexRingIterator {
    if radius == 0 {
        return HexRingIterator {
            current: center,
            radius: 0,
            side: 0,
            step: 0,
            started: false,
        };
    }
    let mut start = center;
    for _ in 0..radius {
        start = start.neighbor(HexDirection::SouthWest);
    }
    HexRingIterator {
        current: start,
        radius,
        side: 0,
        step: 0,
        started: false,
    }
}

pub struct HexRingIterator {
    current: Hex,
    radius: u16,
    side: u32,
    step: u16,
    started: bool,
}

impl Iterator for HexRingIterator {
    type Item = Hex;

    fn next(&mut self) -> Option<Self::Item> {
        if self.radius == 0 {
            if !self.started {
                self.started = true;
                return Some(self.current);
            }
            return None;
        }

        // Walk the ring: emit the current cell, then step, turning a
        // corner every `radius` steps. (The old version stepped side-first,
        // which skewed the ring — emitted cells drifted off-ring and the
        // west-side cells were never visited at all.)
        if self.side >= 6 {
            return None;
        }

        let walk_dirs = [
            HexDirection::East,
            HexDirection::NorthEast,
            HexDirection::NorthWest,
            HexDirection::West,
            HexDirection::SouthWest,
            HexDirection::SouthEast,
        ];

        let out = self.current;
        self.current = self.current.neighbor(walk_dirs[self.side as usize]);
        self.step += 1;
        if self.step >= self.radius {
            self.step = 0;
            self.side += 1;
        }
        Some(out)
    }
}

/// Iterate over all hexes in a spiral from center out to max_radius.
pub fn hex_spiral(center: Hex, max_radius: u16) -> HexSpiralIterator {
    HexSpiralIterator {
        center,
        max_radius,
        current_radius: 0,
        ring_iter: hex_ring(center, 0),
    }
}

pub struct HexSpiralIterator {
    center: Hex,
    max_radius: u16,
    current_radius: u16,
    ring_iter: HexRingIterator,
}

impl Iterator for HexSpiralIterator {
    type Item = Hex;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(h) = self.ring_iter.next() {
            return Some(h);
        }
        self.current_radius += 1;
        if self.current_radius > self.max_radius {
            return None;
        }
        self.ring_iter = hex_ring(self.center, self.current_radius);
        self.ring_iter.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_distance() {
        let a = Hex { q: 0, r: 0 };
        let b = Hex { q: 3, r: -1 };
        assert_eq!(Hex::distance(&a, &b), 3);
    }

    #[test]
    fn hex_distance_symmetric() {
        let a = Hex { q: 2, r: -5 };
        let b = Hex { q: -1, r: 3 };
        assert_eq!(Hex::distance(&a, &b), Hex::distance(&b, &a));
    }

    #[test]
    fn hex_neighbors_count() {
        let center = Hex { q: 5, r: -3 };
        for n in center.neighbors() {
            assert_eq!(Hex::distance(&center, &n), 1);
        }
    }

    #[test]
    fn hex_key_roundtrip() {
        let h = Hex { q: -42, r: 127 };
        let key = h.to_key();
        let h2 = Hex::from_key(key);
        assert_eq!(h, h2);
    }

    #[test]
    fn hex_direction_opposite() {
        assert_eq!(HexDirection::East.opposite(), HexDirection::West);
        assert_eq!(
            HexDirection::NorthEast.opposite(),
            HexDirection::SouthWest
        );
    }

    #[test]
    fn hex_ring_radius_1() {
        let count = hex_ring(Hex::ORIGIN, 1).count();
        assert_eq!(count, 6);
    }

    #[test]
    fn hex_ring_radius_2() {
        let count = hex_ring(Hex::ORIGIN, 2).count();
        assert_eq!(count, 12);
    }

    #[test]
    fn hex_spiral_radius_2() {
        let count = hex_spiral(Hex::ORIGIN, 2).count();
        assert_eq!(count, 19); // 1 + 6 + 12
    }

    #[test]
    fn hex_ring_cells_are_on_the_ring() {
        // Counting isn't enough — the cells must actually sit at the ring
        // distance (regression: the old walk skewed off-ring and skipped
        // the west neighbors entirely).
        for radius in 1..=4u16 {
            let cells: Vec<Hex> = hex_ring(Hex::ORIGIN, radius).collect();
            assert_eq!(cells.len(), (radius * 6) as usize);
            for c in &cells {
                assert_eq!(c.dist_from_origin(), radius, "off-ring cell {c} at radius {radius}");
            }
            let unique: std::collections::HashSet<u32> =
                cells.iter().map(|c| c.to_key()).collect();
            assert_eq!(unique.len(), cells.len(), "duplicate ring cells");
        }
    }

    #[test]
    fn hex_ring_radius_1_is_exactly_the_neighbors() {
        let ring: std::collections::HashSet<u32> =
            hex_ring(Hex::ORIGIN, 1).map(|c| c.to_key()).collect();
        let neighbors: std::collections::HashSet<u32> =
            Hex::ORIGIN.neighbors().iter().map(|c| c.to_key()).collect();
        assert_eq!(ring, neighbors);
    }

    #[test]
    fn hex_spiral_covers_the_disc() {
        let cells: std::collections::HashSet<u32> =
            hex_spiral(Hex::ORIGIN, 3).map(|c| c.to_key()).collect();
        assert_eq!(cells.len(), 37); // 1+6+12+18
        for q in -3i16..=3 {
            for r in -3i16..=3 {
                let h = Hex { q, r };
                if h.dist_from_origin() <= 3 {
                    assert!(cells.contains(&h.to_key()), "spiral missed {h}");
                }
            }
        }
    }
}

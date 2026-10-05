//! The wall and dead wall.
//!
//! The 136 tiles are shuffled once per hand into one array, and every draw
//! after that is just "read the next index". usagi engine uses its own fixed
//! layout (not the physical table's), chosen so every position is a
//! constant:
//!
//! ```text
//! index:   0 ............................ 121 | 122..=125 | 126..=130 | 131..=135
//!          live wall, drawn from index 0 up    | rinshan   | dora ind. | ura ind.
//! ```
//!
//! - Opening deal: 13 tiles to each player from the front of the live wall
//!   (52 tiles), so the dealer's first draw is index 52.
//! - Each kan draws a replacement from the rinshan slots in order, and
//!   reveals the next dora indicator (126 is revealed from the start).
//! - The dead wall always holds 14 tiles, so each kan also shortens the
//!   live wall by one: the last drawable index is `121 - kan_count`.
//!   Tiles left to draw = `122 - kan_count - wall_pos`.
//!
//! The wall stores tile *codes*, so the three red fives are three specific
//! slots.

use usagi_core::Tile;

use crate::rng::Rng;

pub const WALL_SIZE: usize = 136;
pub const LIVE_WALL_END: u8 = 122;
pub const RINSHAN_START: u8 = 122;
pub const DORA_START: u8 = 126;
pub const URA_START: u8 = 131;

/// A full set of 136 tiles in a fixed order, with the right number of red
/// fives: one of the four 5m is code 34 (red) instead of 4, and so on.
pub fn fresh_set(red_fives: u8) -> [Tile; WALL_SIZE] {
    let mut set = [Tile::from_kind(0); WALL_SIZE];
    for (i, t) in set.iter_mut().enumerate() {
        *t = Tile::from_kind((i / 4) as u8);
    }
    // One red per suit for 3; a fourth (second red 5p) for 4.
    let reds: &[(usize, u8)] = &[(4 * 4, 34), (13 * 4, 35), (22 * 4, 36), (13 * 4 + 1, 35)];
    for &(idx, code) in reds.iter().take(red_fives as usize) {
        set[idx] = Tile::from_code(code).unwrap();
    }
    set
}

/// A shuffled wall for `seed`. Fisher-Yates: for i from 135 down to 1,
/// swap index i with a random index in 0..=i.
pub fn shuffled(seed: u64, red_fives: u8) -> [Tile; WALL_SIZE] {
    let mut wall = fresh_set(red_fives);
    let mut rng = Rng::new(seed);
    for i in (1..WALL_SIZE).rev() {
        let j = rng.below(i as u64 + 1) as usize;
        wall.swap(i, j);
    }
    wall
}

/// The `n`-th dora indicator (0 = the one shown at the start).
pub fn dora_indicator(wall: &[Tile; WALL_SIZE], n: u8) -> Tile {
    wall[DORA_START as usize + n as usize]
}

/// The `n`-th ura dora indicator (under the `n`-th dora indicator).
pub fn ura_indicator(wall: &[Tile; WALL_SIZE], n: u8) -> Tile {
    wall[URA_START as usize + n as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    // cargo test -p usagi-engine wall::

    fn code_counts(wall: &[Tile; WALL_SIZE]) -> [u8; 37] {
        let mut c = [0u8; 37];
        for t in wall {
            c[t.code() as usize] += 1;
        }
        c
    }

    #[test]
    fn fresh_set_has_every_tile() {
        let c = code_counts(&fresh_set(3));
        for (kind, &count) in c.iter().enumerate().take(34) {
            let red_suit_five = kind == 4 || kind == 13 || kind == 22;
            let expected = if red_suit_five { 3 } else { 4 };
            assert_eq!(count, expected, "kind {kind}");
        }
        assert_eq!(&c[34..], &[1, 1, 1]);
        let c = code_counts(&fresh_set(0));
        assert_eq!(c[4], 4);
        assert_eq!(&c[34..], &[0, 0, 0]);
    }

    #[test]
    fn shuffle_is_a_permutation_and_reproducible() {
        let a = shuffled(123, 3);
        let b = shuffled(123, 3);
        let c = shuffled(124, 3);
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(code_counts(&a), code_counts(&fresh_set(3)));
    }

    #[test]
    fn indicator_slots() {
        let w = fresh_set(3);
        assert_eq!(dora_indicator(&w, 0), w[126]);
        assert_eq!(dora_indicator(&w, 4), w[130]);
        assert_eq!(ura_indicator(&w, 0), w[131]);
    }
}

//! Hand counts.
//!
//! Almost every hand algorithm (shanten, waits, decomposition) only cares
//! *how many of each kind* you hold, not their order or which copy is red.
//! So instead of a list of tiles, hand logic uses a 34-slot array where
//! slot `k` is the number of tiles of kind `k` (0..=4).

use crate::tile::{NUM_KINDS, Tile};

/// How many of each tile kind a hand holds. Index = kind (0..=33).
///
/// The field is `pub` so fast code can index the array directly
/// (`counts.0[k]`), but prefer the methods while you're learning.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Counts(pub [u8; NUM_KINDS]);

impl Counts {
    /// An empty hand. (Given: same as `Counts::default()`.)
    pub const fn new() -> Counts {
        Counts([0; NUM_KINDS])
    }

    /// Counts a slice of tiles. Red fives count as their plain kind.
    pub fn from_tiles(tiles: &[Tile]) -> Counts {
        let mut c = Counts::new();
        for t in tiles {
            c.add(t.kind());
        }
        c
    }

    /// How many tiles of `kind` the hand holds.
    pub fn get(&self, kind: u8) -> u8 {
        self.0[kind as usize]
    }

    /// Adds one tile of `kind`. Panics (in debug builds) if that would make 5.
    pub fn add(&mut self, kind: u8) {
        debug_assert!(self.0[kind as usize] < 4, "fifth copy of kind {kind}");
        self.0[kind as usize] += 1;
    }

    /// Removes one tile of `kind`. Panics (in debug builds) if there is none.
    pub fn remove(&mut self, kind: u8) {
        debug_assert!(self.0[kind as usize] > 0, "no kind {kind} to remove");
        self.0[kind as usize] -= 1;
    }

    /// Total number of tiles in the hand.
    pub fn total(&self) -> u32 {
        self.0.iter().map(|&c| c as u32).sum()
    }

    /// The kinds present at least once, in increasing order.
    ///
    /// Returning `impl Iterator<Item = u8> + '_` means "some iterator type
    /// I don't want to name, which borrows `self`". Build it with
    /// `(0..NUM_KINDS as u8).filter(...)`.
    pub fn kinds(&self) -> impl Iterator<Item = u8> + '_ {
        (0..NUM_KINDS as u8).filter(move |&k| self.0[k as usize] > 0)
    }
}

// `#[derive(Default)]` doesn't work for arrays longer than 32, so this
// one is written out by hand.
impl Default for Counts {
    fn default() -> Self {
        Counts::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // cargo test -p usagi-core counts::

    #[test]
    fn add_remove_get() {
        let mut c = Counts::new();
        c.add(5);
        c.add(5);
        c.add(33);
        assert_eq!(c.get(5), 2);
        assert_eq!(c.get(33), 1);
        assert_eq!(c.get(0), 0);
        assert_eq!(c.total(), 3);
        c.remove(5);
        assert_eq!(c.get(5), 1);
        assert_eq!(c.total(), 2);
        assert_eq!(c.kinds().collect::<Vec<_>>(), vec![5, 33]);
    }

    // These two check that a mistake panics. They first do something
    // legal, so an unrelated panic can't make them pass.

    #[test]
    fn fifth_copy_panics() {
        let mut c = Counts::new();
        for _ in 0..4 {
            c.add(0);
        }
        let result = std::panic::catch_unwind(move || c.add(0));
        assert!(result.is_err(), "adding a fifth copy must panic");
    }

    #[test]
    fn remove_missing_panics() {
        let mut c = Counts::new();
        c.add(7);
        c.remove(7);
        let result = std::panic::catch_unwind(move || c.remove(7));
        assert!(
            result.is_err(),
            "removing a tile that isn't there must panic"
        );
    }

    #[test]
    fn from_tiles_folds_reds() {
        use crate::tile::Suit;
        let tiles = [
            Tile::from_kind(4),
            Tile::red_five(Suit::Man),
            Tile::from_kind(27),
        ];
        let c = Counts::from_tiles(&tiles);
        assert_eq!(c.get(4), 2);
        assert_eq!(c.get(27), 1);
        assert_eq!(c.total(), 3);
    }
}

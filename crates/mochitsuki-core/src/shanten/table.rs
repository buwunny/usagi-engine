//! Fast shanten with lookup tables.
//!
//! The reference version spends almost all its time in `group_entry`,
//! recomputed for every hand. But a group only has 5^9 = 1,953,125
//! possible count patterns (each of its 9 kinds held 0..=4 times), and
//! honors 5^7 = 78,125. So compute every group's entry once, store them in
//! a table indexed by the pattern, and a shanten call becomes four table
//! lookups plus a tiny combine step.
//!
//! The pattern's index is the counts read as a base-5 number:
//! `c[0] + 5*c[1] + 25*c[2] + ...`.
//!
//! Computing 2 million entries with the reference `group_entry` would take
//! minutes, so `build` fills the table bottom-up instead.

use std::sync::OnceLock;

use super::GroupEntry;
use crate::counts::Counts;

/// Number of count patterns for a 9-kind suit group.
pub const SUIT_PATTERNS: usize = 1_953_125; // 5^9
/// Number of count patterns for the 7-kind honor group.
pub const HONOR_PATTERNS: usize = 78_125; // 5^7

/// The two lookup tables.
pub struct Tables {
    /// One entry per suit pattern (shared by manzu, pinzu and souzu).
    pub suit: Vec<GroupEntry>,
    /// One entry per honor pattern.
    pub honor: Vec<GroupEntry>,
}

/// Base-5 index of a group's counts.
pub fn pattern_index(group: &[u8]) -> usize {
    group.iter().rev().fold(0usize, |i, &c| i * 5 + c as usize)
}

/// Builds both tables from scratch.
pub fn build() -> Tables {
    Tables {
        suit: build_group(false),
        honor: build_group(true),
    }
}

/// Builds one group's table with the downward-closure method.
///
/// Every placement in slot `s` has the same size `3i (+2)`, so the distance
/// is `size - max overlap`, and the max overlap with `h` is the largest
/// `|q|` over vectors `q <= h` that lie under some placement (the downward
/// closure). That is a DP over patterns in increasing index order.
fn build_group(is_honor: bool) -> Vec<GroupEntry> {
    let n = if is_honor { 7 } else { 9 };
    let size = 5usize.pow(n as u32);
    let mut pow5 = [1usize; 9];
    for k in 1..9 {
        pow5[k] = pow5[k - 1] * 5;
    }
    // closure[h] bit s: pattern h lies under some placement of slot s.
    let mut closure = vec![0u16; size];
    for (sets, pair, v) in super::reference::placements(is_honor) {
        let slot = sets as usize + if pair { 5 } else { 0 };
        closure[pattern_index(&v)] |= 1 << slot;
    }
    let digits = |mut h: usize| {
        let mut d = [0u8; 9];
        for x in d.iter_mut().take(n) {
            *x = (h % 5) as u8;
            h /= 5;
        }
        d
    };
    for h in (0..size).rev() {
        let d = digits(h);
        let mut bits = closure[h];
        for k in 0..n {
            if d[k] < 4 {
                bits |= closure[h + pow5[k]];
            }
        }
        // Only need to propagate from one step up: higher patterns were
        // already folded into their own one-step-up neighbours.
        closure[h] = bits;
    }
    // best[h][s] = max |q| for q <= h, q in closure of slot s.
    let mut best: Vec<[u8; 10]> = vec![[0u8; 10]; size];
    for h in 0..size {
        let d = digits(h);
        let total: u8 = d.iter().sum();
        let mut b = [0u8; 10];
        for k in 0..n {
            if d[k] > 0 {
                let prev = &best[h - pow5[k]];
                for (x, &y) in b.iter_mut().zip(prev) {
                    *x = (*x).max(y);
                }
            }
        }
        for (s, x) in b.iter_mut().enumerate() {
            if closure[h] >> s & 1 == 1 {
                *x = total;
            }
        }
        best[h] = b;
    }
    best.into_iter()
        .map(|b| {
            let mut e = [0u8; 10];
            for (s, x) in e.iter_mut().enumerate() {
                let sz = 3 * (s % 5) as u8 + if s >= 5 { 2 } else { 0 };
                *x = sz - b[s];
            }
            e
        })
        .collect()
}

/// The tables, built on first use and shared after that.
///
/// `OnceLock` runs `build()` the first time anyone calls this, from any
/// thread, and hands every caller a `&'static Tables` afterwards.
pub fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(build)
}

/// Standard-shape shanten from the tables.
pub fn standard(counts: &Counts, melds: u8) -> i8 {
    let t = tables();
    let c = &counts.0;
    let entries = [
        t.suit[pattern_index(&c[0..9])],
        t.suit[pattern_index(&c[9..18])],
        t.suit[pattern_index(&c[18..27])],
        t.honor[pattern_index(&c[27..34])],
    ];
    super::combine(&entries, melds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shanten::reference;
    use proptest::prelude::*;

    // cargo test -p mochitsuki-core shanten::table

    #[test]
    fn index_is_base_5() {
        assert_eq!(pattern_index(&[0; 9]), 0);
        assert_eq!(pattern_index(&[1, 0, 0, 0, 0, 0, 0, 0, 0]), 1);
        assert_eq!(pattern_index(&[0, 1, 0, 0, 0, 0, 0, 0, 0]), 5);
        assert_eq!(pattern_index(&[4; 9]), SUIT_PATTERNS - 1);
        assert_eq!(pattern_index(&[4; 7]), HONOR_PATTERNS - 1);
    }

    /// A random hand: `n` distinct physical tiles out of 136, as counts.
    fn random_hand(n: usize) -> impl Strategy<Value = Counts> {
        prop::sample::subsequence((0u8..136).collect::<Vec<_>>(), n).prop_map(|tiles| {
            let mut c = Counts::new();
            for t in tiles {
                c.0[(t / 4) as usize] += 1;
            }
            c
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(300))]

        #[test]
        fn matches_reference_13(c in random_hand(13)) {
            prop_assert_eq!(standard(&c, 0), reference::standard(&c, 0));
        }

        #[test]
        fn matches_reference_14(c in random_hand(14)) {
            prop_assert_eq!(standard(&c, 0), reference::standard(&c, 0));
        }

        #[test]
        fn matches_reference_with_melds(c in random_hand(7)) {
            // 7 concealed tiles = 13-tile hand with two called sets.
            prop_assert_eq!(standard(&c, 2), reference::standard(&c, 2));
        }
    }

    /// A bigger check than the proptests: 200,000 random hands. Slow
    /// (the reference is slow), so it only runs when asked:
    ///     cargo test --release -p mochitsuki-core many_random -- --ignored
    /// The plan's 1M-hand check comes later, from fuzzing against
    /// riichienv-core, which is fast on both sides.
    #[test]
    #[ignore]
    fn many_random_hands_match_reference() {
        // A tiny xorshift RNG, so this test needs no extra crates.
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for i in 0..200_000 {
            let mut wall: Vec<u8> = (0..136).collect();
            let size = 13 + (i % 2);
            let mut c = Counts::new();
            for j in 0..size {
                let pick = j + (next() % (136 - j as u64)) as usize;
                wall.swap(j, pick);
                c.0[(wall[j] / 4) as usize] += 1;
            }
            assert_eq!(standard(&c, 0), reference::standard(&c, 0), "hand {c:?}");
        }
    }
}

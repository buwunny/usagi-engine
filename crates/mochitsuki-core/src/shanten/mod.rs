//! Shanten: how far a hand is from tenpai.
//!
//! *Shanten* counts how many useful tiles you still need before you are
//! *tenpai* (one tile from winning):
//!
//! | shanten | meaning                                   |
//! |---------|-------------------------------------------|
//! | -1      | the hand is complete (only with 14 tiles) |
//! | 0       | tenpai                                    |
//! | 1       | iishanten: one useful tile from tenpai    |
//! | n       | n useful tiles from tenpai                |
//!
//! A hand can win in three shapes, and its shanten is the minimum of the
//! three:
//! - standard: four sets plus a pair ([`standard`]),
//! - seven pairs, *chiitoitsu* ([`chiitoitsu`]),
//! - thirteen orphans, *kokushi musou* ([`kokushi`]).
//!
//! Chiitoitsu and kokushi are closed-hand only, so they only count when the
//! player has made no calls.
//!
//! ## Input convention
//!
//! Every function takes the *concealed* tiles as [`Counts`] plus `melds`, the
//! number of called sets (each called pon/chi/kan counts as one set already
//! done). The concealed tiles number `13 - 3 * melds` (waiting for a draw)
//! or `14 - 3 * melds` (just drew). A kan is still one set here: its fourth
//! tile was replaced by a dead-wall draw.
//!
//! The same formulas work for 13 and 14 tiles: a 14-tile hand that is
//! complete gets -1.

pub mod reference;
pub mod table;

use crate::counts::Counts;

/// Distances for one group (one suit, or the honors).
///
/// - `entry[i]` for `i` in 0..=4: fewest tiles to add so the group holds
///   `i` complete sets.
/// - `entry[5 + i]`: fewest tiles to add so it holds `i` sets **and** a pair.
///
/// For an empty suit that is `[0, 3, 6, 9, 12, 2, 5, 8, 11, 14]`.
pub type GroupEntry = [u8; 10];

/// Combines the four groups' entries into a shanten number.
///
/// The hand needs `4 - melds` sets and exactly one pair, shared out among
/// the four groups in any way. Try every split, sum the distances, keep
/// the smallest, subtract 1.
pub fn combine(entries: &[GroupEntry; 4], melds: u8) -> i8 {
    let need = 4usize.saturating_sub(melds as usize);
    let mut best = u32::MAX;
    for a in 0..=need {
        for b in 0..=need - a {
            for c in 0..=need - a - b {
                let d = need - a - b - c;
                if d > 4 {
                    continue;
                }
                let sets = [a, b, c, d];
                for pg in 0..4 {
                    let mut sum = 0u32;
                    for g in 0..4 {
                        let idx = if g == pg { 5 + sets[g] } else { sets[g] };
                        sum += entries[g][idx] as u32;
                    }
                    best = best.min(sum);
                }
            }
        }
    }
    best as i8 - 1
}

/// Shanten of the hand over all three winning shapes.
pub fn shanten(counts: &Counts, melds: u8) -> i8 {
    let s = standard(counts, melds);
    if melds == 0 {
        s.min(chiitoitsu(counts)).min(kokushi(counts))
    } else {
        s
    }
}

/// Shanten for the standard shape (four sets and a pair).
///
/// This is the one that gets called millions of times, so it uses the fast
/// lookup tables from [`table`]. Until those exist (step 2.3), you can make
/// it call `reference::standard` so the rest of the crate works.
pub fn standard(counts: &Counts, melds: u8) -> i8 {
    table::standard(counts, melds)
}

/// Shanten for seven pairs. Tenhou rule: the seven pairs must be seven
/// *different* kinds, so four of a kind is not two pairs.
pub fn chiitoitsu(counts: &Counts) -> i8 {
    let pairs = counts.0.iter().filter(|&&c| c >= 2).count() as i8;
    let kinds = counts.0.iter().filter(|&&c| c >= 1).count() as i8;
    6 - pairs + (7 - kinds).max(0)
}

/// Shanten for thirteen orphans: one of each of the 13 terminal and honor
/// kinds, plus a second copy of any one of them.
pub fn kokushi(counts: &Counts) -> i8 {
    const YAOCHU: [usize; 13] = [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];
    let distinct = YAOCHU.iter().filter(|&&k| counts.0[k] >= 1).count() as i8;
    let pair = YAOCHU.iter().any(|&k| counts.0[k] >= 2);
    13 - distinct - pair as i8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::counts_of;

    // cargo test -p mochitsuki-core shanten::tests

    fn sh(hand: &str) -> i8 {
        shanten(&counts_of(hand), 0)
    }

    #[test]
    fn group_entries() {
        use reference::group_entry;
        assert_eq!(
            group_entry(&[0; 9], false),
            [0, 3, 6, 9, 12, 2, 5, 8, 11, 14]
        );
        assert_eq!(
            group_entry(&[0; 7], true),
            [0, 3, 6, 9, 12, 2, 5, 8, 11, 14]
        );
        // 123m: one set already there.
        assert_eq!(
            group_entry(&[1, 1, 1, 0, 0, 0, 0, 0, 0], false),
            [0, 0, 3, 6, 9, 1, 2, 5, 8, 11]
        );
        // 11z + 7z: a pair is free, and 11z is one tile from a triplet.
        assert_eq!(
            group_entry(&[2, 0, 0, 0, 0, 0, 1], true),
            [0, 1, 3, 6, 9, 0, 2, 5, 8, 11]
        );
        // 11233m + 9999m.
        assert_eq!(
            group_entry(&[1, 1, 2, 0, 0, 0, 0, 0, 4], false),
            [0, 0, 0, 2, 4, 0, 0, 1, 3, 6]
        );
    }

    #[test]
    fn combine_examples() {
        let empty = [0, 3, 6, 9, 12, 2, 5, 8, 11, 14];
        // Nothing at all: 4 sets (12 tiles) + pair (2) = 14 to add, minus 1.
        assert_eq!(combine(&[empty; 4], 0), 13);
        // Four calls already made: just the pair, 2 tiles, minus 1.
        assert_eq!(combine(&[empty; 4], 4), 1);
    }

    #[test]
    fn reference_standard() {
        let s = |h: &str| reference::standard(&counts_of(h), 0);
        assert_eq!(s("123m456p789s1122z"), 0);
        assert_eq!(s("123m456p789s11222z"), -1);
        assert_eq!(s("147m258p369s1234z"), 8);
    }

    #[test]
    fn chiitoitsu_shanten() {
        assert_eq!(chiitoitsu(&counts_of("1199m1199p1199s1z2z")), 0);
        assert_eq!(chiitoitsu(&counts_of("11223344m556677p")), -1);
        assert_eq!(chiitoitsu(&counts_of("147m258p369s1234z")), 6);
        // Four of a kind is not two pairs: 1111m counts as one pair.
        assert_eq!(chiitoitsu(&counts_of("1111m2233p4455s6z")), 2);
    }

    #[test]
    fn kokushi_shanten() {
        assert_eq!(kokushi(&counts_of("19m19p19s1234567z")), 0); // 13-sided wait
        assert_eq!(kokushi(&counts_of("19m19p19s1234567z1z")), -1); // complete
        assert_eq!(kokushi(&counts_of("19m19p19s123456z5m")), 1);
        assert_eq!(kokushi(&counts_of("19m19p19s123455z5m")), 1);
        assert_eq!(kokushi(&counts_of("2345678m2345678p")), 13);
    }

    #[test]
    fn standard_shanten() {
        let s = |h: &str| standard(&counts_of(h), 0);
        assert_eq!(s("123m456p789s1122z"), 0);
        assert_eq!(s("123m456p789s11222z"), -1);
        assert_eq!(s("2234m567p789s345s"), 0);
        assert_eq!(s("1112345678999m"), 0); // nine-sided wait
        assert_eq!(s("13579m13579p135s"), 4);
        assert_eq!(s("1469m1469p1469s1z"), 5);
        assert_eq!(s("147m258p369s1234z"), 8); // the worst possible standard shanten
        assert_eq!(s("1111m2222p3333s4z"), 2);
    }

    #[test]
    fn standard_with_melds() {
        // One called set: 10 concealed tiles.
        assert_eq!(standard(&counts_of("234m55p678s1122z"), 1), 0);
        // Four called sets: a lone tile waiting for its pair (tanki).
        assert_eq!(standard(&counts_of("5z"), 4), 0);
        assert_eq!(standard(&counts_of("55z"), 4), -1);
        assert_eq!(standard(&counts_of("45z"), 4), 0);
    }

    #[test]
    fn overall_minimum() {
        assert_eq!(sh("1199m1199p1199s1z2z"), 0); // chiitoitsu wins
        assert_eq!(sh("19m19p19s1234567z"), 0); // kokushi wins
        assert_eq!(sh("147m258p369s1234z"), 6); // chiitoitsu beats standard 8
        assert_eq!(sh("123m456p789s1122z"), 0);
        // With a call, chiitoitsu and kokushi don't count.
        assert_eq!(shanten(&counts_of("1199m1199p11s"), 1), 2);
    }
}

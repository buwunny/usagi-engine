//! The slow, obviously-correct shanten.
//!
//! This exists to check the fast version against. Write it for clarity,
//! not speed: if the two ever disagree, you want to be sure this one is
//! right.
//!
//! ## The definition it implements
//!
//! Split the 34 kinds into four *groups*: manzu, pinzu, souzu (9 kinds
//! each) and honors (7 kinds). Sets never cross groups, so a winning hand
//! is just "some sets and maybe the pair in each group".
//!
//! For one group, its *distance* to "`i` sets" (or "`i` sets plus the
//! pair") is the fewest tiles you would have to **add** to that group so it
//! contains that many sets. Tiles you'd have to throw away don't cost
//! anything: you get to discard one tile per draw anyway.
//!
//! Then for the whole hand with `melds` called sets:
//!
//! ```text
//! shanten = (min over ways to share out (4 - melds) sets and 1 pair
//!            among the four groups of the summed distances) - 1
//! ```
//!
//! The `- 1` is because the last tile you need wins the hand rather than
//! making it tenpai. A complete 14-tile hand has distance 0, so -1.

use super::GroupEntry;
use crate::counts::Counts;

/// Every set and pair you could place inside one group, as a list of
/// count arrays.
///
/// Returns, for one group of `n` kinds (9 for a suit, 7 for honors; honors
/// have no sequences), every way to place 0..=4 sets plus optionally one
/// pair, without any kind going over 4 copies. Each entry is
/// `(number_of_sets, has_pair, counts_needed)`.
///
/// There are a few thousand of these for a suit, which is small enough to
/// try every one.
pub fn placements(is_honor: bool) -> Vec<(u8, bool, Vec<u8>)> {
    let n = if is_honor { 7 } else { 9 };
    // Set shapes: triplets 0..n, then sequences 0..7 (suits only).
    let mut shapes: Vec<Vec<u8>> = Vec::new();
    for k in 0..n {
        let mut v = vec![0u8; n];
        v[k] = 3;
        shapes.push(v);
    }
    if !is_honor {
        for k in 0..7 {
            let mut v = vec![0u8; n];
            v[k] = 1;
            v[k + 1] = 1;
            v[k + 2] = 1;
            shapes.push(v);
        }
    }
    let mut out = Vec::new();
    fn rec(
        shapes: &[Vec<u8>],
        start: usize,
        cur: &mut Vec<u8>,
        sets: u8,
        out: &mut Vec<(u8, bool, Vec<u8>)>,
    ) {
        out.push((sets, false, cur.clone()));
        for k in 0..cur.len() {
            if cur[k] + 2 <= 4 {
                let mut v = cur.clone();
                v[k] += 2;
                out.push((sets, true, v));
            }
        }
        if sets == 4 {
            return;
        }
        for (i, sh) in shapes.iter().enumerate().skip(start) {
            if cur.iter().zip(sh).all(|(&a, &b)| a + b <= 4) {
                for (a, &b) in cur.iter_mut().zip(sh) {
                    *a += b;
                }
                rec(shapes, i, cur, sets + 1, out);
                for (a, &b) in cur.iter_mut().zip(sh) {
                    *a -= b;
                }
            }
        }
    }
    rec(&shapes, 0, &mut vec![0u8; n], 0, &mut out);
    out
}

/// Distances for one group: entry `i` (0..=4) is the distance to `i` sets,
/// entry `5 + i` the distance to `i` sets plus a pair. See [`GroupEntry`].
///
/// `group` is that group's slice of the counts (9 or 7 numbers).
pub fn group_entry(group: &[u8], is_honor: bool) -> GroupEntry {
    use std::sync::OnceLock;
    // Placements flattened into fixed arrays, computed once.
    static SUIT: OnceLock<Vec<(usize, [u8; 9])>> = OnceLock::new();
    static HONOR: OnceLock<Vec<(usize, [u8; 9])>> = OnceLock::new();
    let flat = |h: bool| {
        placements(h)
            .into_iter()
            .map(|(sets, pair, v)| {
                let mut a = [0u8; 9];
                a[..v.len()].copy_from_slice(&v);
                (sets as usize + if pair { 5 } else { 0 }, a)
            })
            .collect::<Vec<_>>()
    };
    let list = if is_honor {
        HONOR.get_or_init(|| flat(true))
    } else {
        SUIT.get_or_init(|| flat(false))
    };
    let mut held = [0u8; 9];
    held[..group.len()].copy_from_slice(group);
    let mut entry = [u8::MAX; 10];
    for (slot, need) in list {
        let mut d = 0u8;
        for k in 0..9 {
            d += need[k].saturating_sub(held[k]);
        }
        if d < entry[*slot] {
            entry[*slot] = d;
        }
    }
    entry
}

/// Standard-shape shanten, the slow way.
pub fn standard(counts: &Counts, melds: u8) -> i8 {
    let c = &counts.0;
    let entries = [
        group_entry(&c[0..9], false),
        group_entry(&c[9..18], false),
        group_entry(&c[18..27], false),
        group_entry(&c[27..34], true),
    ];
    super::combine(&entries, melds)
}

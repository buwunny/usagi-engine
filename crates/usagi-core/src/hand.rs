//! Melds, winning hands and their decompositions.
//!
//! Scoring a win starts by asking "how can these 14 tiles be read as four
//! sets and a pair?". Often there's exactly one reading, but not always:
//! `111222333m` is three triplets *or* three identical sequences, and the
//! two readings score differently. Tenhou scores every reading and pays the
//! best one, so we need all of them.
//!
//! Vocabulary used in this module:
//! - **set** (*mentsu*): a sequence (*shuntsu*, 3 consecutive tiles of a
//!   suit, like 456p) or a triplet (*koutsu*, 3 identical tiles). A kan
//!   (*kantsu*, 4 identical) also counts as one set.
//! - **pair** (*jantou*): 2 identical tiles; a standard hand has exactly one.
//! - **meld**: a set made by calling another player's discard (chi, pon,
//!   open kan), plus the closed kan (*ankan*), which is declared but stays
//!   closed.
//! - **wait**: the shape the winning tile completed (see [`Wait`]).

use crate::counts::Counts;
use crate::tile::Tile;

/// The kinds of meld. Chi and pon take a discard; kans have several flavors.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MeldKind {
    /// A sequence called from the player on your left.
    Chi,
    /// A triplet called from anyone.
    Pon,
    /// Open kan made by calling a discard while holding three (*daiminkan*).
    Daiminkan,
    /// Open kan made by adding your drawn 4th tile to your own pon (*kakan*,
    /// also *shouminkan*).
    Kakan,
    /// Closed kan of four tiles all from your own hand (*ankan*).
    Ankan,
}

impl MeldKind {
    /// Whether this meld opens the hand. Every meld except the closed kan does.
    pub fn is_open(self) -> bool {
        self != MeldKind::Ankan
    }

    pub fn is_kan(self) -> bool {
        matches!(
            self,
            MeldKind::Daiminkan | MeldKind::Kakan | MeldKind::Ankan
        )
    }
}

/// A meld as scoring needs it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Meld {
    pub kind: MeldKind,
    /// For chi: the kind of the lowest tile (345m -> kind of 3m).
    /// For pon and kans: the tile kind.
    pub first: u8,
    /// How many red fives are in the meld (0 or 1).
    pub reds: u8,
}

impl Meld {
    /// Builds a meld from its tiles, e.g. `Meld::from_tiles(MeldKind::Pon, &tiles_of("777z"))`.
    ///
    /// Panics if the tiles don't form that kind of meld (wrong count,
    /// not a sequence, not identical).
    pub fn from_tiles(kind: MeldKind, tiles: &[Tile]) -> Meld {
        let mut kinds: Vec<u8> = tiles.iter().map(|t| t.kind()).collect();
        kinds.sort();
        let reds = tiles.iter().filter(|t| t.is_red()).count() as u8;
        match kind {
            MeldKind::Chi => {
                assert_eq!(kinds.len(), 3, "chi needs 3 tiles");
                assert!(kinds[0] < 27, "chi must be suited");
                assert!(
                    kinds[0] % 9 <= 6 && kinds[1] == kinds[0] + 1 && kinds[2] == kinds[0] + 2,
                    "chi must be a sequence"
                );
            }
            MeldKind::Pon => {
                assert_eq!(kinds.len(), 3, "pon needs 3 tiles");
                assert!(
                    kinds.iter().all(|&k| k == kinds[0]),
                    "pon must be identical"
                );
            }
            _ => {
                assert_eq!(kinds.len(), 4, "kan needs 4 tiles");
                assert!(
                    kinds.iter().all(|&k| k == kinds[0]),
                    "kan must be identical"
                );
            }
        }
        Meld {
            kind,
            first: kinds[0],
            reds,
        }
    }
}

/// A set or pair inside a decomposition of the concealed tiles.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Block {
    /// Three consecutive kinds starting at `first` (same suit, no honors).
    Sequence {
        first: u8,
    },
    Triplet {
        kind: u8,
    },
    Pair {
        kind: u8,
    },
}

/// One way to split the concealed tiles of a *complete* standard hand into
/// sets and exactly one pair.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Decomposition {
    pub pair: u8,
    /// The concealed sets (not the called melds), sorted so two equal
    /// decompositions compare equal.
    pub sets: Vec<Block>,
}

/// Every way to read `concealed` as `concealed.total() / 3` sets plus one
/// pair. Empty if there is none (the hand isn't a standard win).
///
/// `concealed` has `14 - 3 * melds` tiles, winning tile included.
pub fn decompositions(concealed: &Counts) -> Vec<Decomposition> {
    fn strip(c: &mut Counts, sets: &mut Vec<Block>, pair: u8, out: &mut Vec<Decomposition>) {
        let Some(k) = (0..34u8).find(|&k| c.0[k as usize] > 0) else {
            let mut sorted = sets.clone();
            sorted.sort();
            out.push(Decomposition { pair, sets: sorted });
            return;
        };
        let ku = k as usize;
        if c.0[ku] >= 3 {
            c.0[ku] -= 3;
            sets.push(Block::Triplet { kind: k });
            strip(c, sets, pair, out);
            sets.pop();
            c.0[ku] += 3;
        }
        if k < 27 && k % 9 <= 6 && c.0[ku + 1] > 0 && c.0[ku + 2] > 0 {
            c.0[ku] -= 1;
            c.0[ku + 1] -= 1;
            c.0[ku + 2] -= 1;
            sets.push(Block::Sequence { first: k });
            strip(c, sets, pair, out);
            sets.pop();
            c.0[ku] += 1;
            c.0[ku + 1] += 1;
            c.0[ku + 2] += 1;
        }
    }
    let mut out = Vec::new();
    if concealed.total() % 3 != 2 {
        return out;
    }
    let mut c = *concealed;
    for p in 0..34u8 {
        if c.0[p as usize] >= 2 {
            c.0[p as usize] -= 2;
            strip(&mut c, &mut Vec::new(), p, &mut out);
            c.0[p as usize] += 2;
        }
    }
    out.sort_by(|a, b| (a.pair, &a.sets).cmp(&(b.pair, &b.sets)));
    out.dedup();
    out
}

/// The shape the winning tile completed. Fu and pinfu depend on it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Wait {
    /// Two-sided: 45 waiting on 3 or 6. (0 fu)
    Ryanmen,
    /// Middle: 46 waiting on 5. (+2 fu)
    Kanchan,
    /// Edge: 12 waiting on 3, or 89 waiting on 7. (+2 fu)
    Penchan,
    /// Two pairs, one becomes a triplet: 55 + 77 waiting on 5 or 7. (0 fu)
    Shanpon,
    /// Single tile waiting for its pair. (+2 fu)
    Tanki,
}

/// A set in a finished hand, with whether it counts as open.
///
/// A triplet completed by **ron** counts as open (for fu and for
/// sanankou/suuankou), even though it was in a closed hand. That rule is
/// the reason this type exists separately from [`Block`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Set {
    Sequence { first: u8, open: bool },
    Triplet { kind: u8, open: bool },
    Kan { kind: u8, open: bool },
}

/// One complete reading of a standard winning hand: every set (called
/// melds included), the pair, and the wait. Yaku and fu are computed from this.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Reading {
    /// Always 4 sets.
    pub sets: Vec<Set>,
    pub pair: u8,
    pub wait: Wait,
    /// True when no meld opens the hand (closed kans are fine).
    pub closed: bool,
}

/// Every reading of a standard winning hand: each decomposition, times
/// each block the winning tile could have completed.
///
/// `concealed` includes the winning tile. `ron` is true when the winning
/// tile came from another player's discard (or a kan, for chankan).
pub fn readings(concealed: &Counts, melds: &[Meld], win_kind: u8, ron: bool) -> Vec<Reading> {
    let closed = melds.iter().all(|m| !m.kind.is_open());
    let meld_sets: Vec<Set> = melds
        .iter()
        .map(|m| match m.kind {
            MeldKind::Chi => Set::Sequence {
                first: m.first,
                open: true,
            },
            MeldKind::Pon => Set::Triplet {
                kind: m.first,
                open: true,
            },
            k => Set::Kan {
                kind: m.first,
                open: k.is_open(),
            },
        })
        .collect();
    let mut out = Vec::new();
    for d in decompositions(concealed) {
        let base: Vec<Set> = d
            .sets
            .iter()
            .map(|b| match *b {
                Block::Sequence { first } => Set::Sequence { first, open: false },
                Block::Triplet { kind } => Set::Triplet { kind, open: false },
                Block::Pair { .. } => unreachable!(),
            })
            .collect();
        let mut push = |sets: Vec<Set>, wait: Wait| {
            let mut all = meld_sets.clone();
            all.extend(sets);
            let r = Reading {
                sets: all,
                pair: d.pair,
                wait,
                closed,
            };
            if !out.contains(&r) {
                out.push(r);
            }
        };
        if d.pair == win_kind {
            push(base.clone(), Wait::Tanki);
        }
        for (i, b) in d.sets.iter().enumerate() {
            match *b {
                Block::Triplet { kind } if kind == win_kind => {
                    let mut sets = base.clone();
                    sets[i] = Set::Triplet { kind, open: ron };
                    push(sets, Wait::Shanpon);
                }
                Block::Sequence { first } if first <= win_kind && win_kind <= first + 2 => {
                    let pos = win_kind - first;
                    let num = first % 9; // 0-based number of the lowest tile
                    let wait = match pos {
                        1 => Wait::Kanchan,
                        0 if num == 6 => Wait::Penchan, // 89 waiting on 7
                        2 if num == 0 => Wait::Penchan, // 12 waiting on 3
                        _ => Wait::Ryanmen,
                    };
                    push(base.clone(), wait);
                }
                _ => {}
            }
        }
    }
    out
}

/// True if the 14 concealed tiles are seven different pairs.
pub fn is_chiitoitsu(concealed: &Counts) -> bool {
    concealed.total() == 14 && concealed.0.iter().filter(|&&c| c == 2).count() == 7
}

/// True if the 14 concealed tiles are one of each terminal and honor plus
/// one extra of any of them.
pub fn is_kokushi(concealed: &Counts) -> bool {
    concealed.total() == 14 && crate::shanten::kokushi(concealed) == -1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{counts_of, tiles_of};

    // cargo test -p usagi-core hand::

    #[test]
    fn meld_shapes() {
        let chi = Meld::from_tiles(MeldKind::Chi, &tiles_of("534p"));
        assert_eq!((chi.first, chi.reds), (11, 0)); // lowest is 3p = kind 11
        let pon = Meld::from_tiles(MeldKind::Pon, &tiles_of("777z"));
        assert_eq!((pon.first, pon.reds), (33, 0));
        let kan = Meld::from_tiles(MeldKind::Daiminkan, &tiles_of("5505s"));
        assert_eq!((kan.first, kan.reds), (22, 1));
        assert!(MeldKind::Pon.is_open() && !MeldKind::Ankan.is_open());
        assert!(MeldKind::Kakan.is_kan() && !MeldKind::Chi.is_kan());
    }

    #[test]
    fn bad_melds_panic() {
        // A good one first, so an unrelated panic can't pass this test.
        Meld::from_tiles(MeldKind::Chi, &tiles_of("123m"));
        for (kind, tiles) in [
            (MeldKind::Chi, "135m"),
            (MeldKind::Chi, "123z"),
            (MeldKind::Chi, "891m"),
            (MeldKind::Pon, "556m"),
            (MeldKind::Ankan, "111z"),
        ] {
            let tiles = tiles_of(tiles);
            let result = std::panic::catch_unwind(|| Meld::from_tiles(kind, &tiles));
            assert!(result.is_err(), "{kind:?} {tiles:?} should be rejected");
        }
    }

    #[test]
    fn single_decomposition() {
        let d = decompositions(&counts_of("123m456p789s11122z"));
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].pair, 28); // 22z = South
    }

    #[test]
    fn triplets_or_sequences() {
        // 111222333m 456p 77s: three triplets, or three 123m sequences.
        let d = decompositions(&counts_of("111222333m456p77s"));
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn pair_choice_matters() {
        // 11123m could start as 111 + 23 or as 11 + 123. Only the second
        // completes, so there is exactly one decomposition.
        assert_eq!(decompositions(&counts_of("11123m456p789s123s")).len(), 1);
        // Not a winning hand at all.
        assert!(decompositions(&counts_of("13579m13579p1357s")).is_empty());
    }

    #[test]
    fn nine_gates_shape_has_one_decomposition() {
        // A chuuren-shaped hand looks like it should split many ways, but
        // only 111 234 55 678 999 works. Convince yourself on paper.
        let d = decompositions(&counts_of("11123455678999m"));
        assert_eq!(d.len(), 1);
        for dec in &d {
            assert_eq!(dec.sets.len(), 4);
        }
    }

    #[test]
    fn wait_shapes() {
        let waits = |hand: &str, win: u8| -> Vec<Wait> {
            let mut w: Vec<Wait> = readings(&counts_of(hand), &[], win, true)
                .iter()
                .map(|r| r.wait)
                .collect();
            w.sort_by_key(|w| *w as u8);
            w.dedup();
            w
        };
        // Win on 3m completing 123m: penchan (12 waited only on 3).
        assert_eq!(waits("123m456p789s11z555z", 2), vec![Wait::Penchan]);
        // 456m: winning on 4m (from 56m) is ryanmen, on 5m (from 46m) kanchan.
        assert_eq!(waits("456m456p789s11z555z", 3), vec![Wait::Ryanmen]);
        assert_eq!(waits("456m456p789s11z555z", 4), vec![Wait::Kanchan]);
        assert_eq!(waits("456m456p789s11z555z", 27), vec![Wait::Tanki]);
        assert_eq!(waits("456m456p789s11z555z", 31), vec![Wait::Shanpon]);
        // 789m won on 7m: penchan (89 waits only on 7).
        assert_eq!(waits("789m456p789s11z555z", 6), vec![Wait::Penchan]);
        // 3m in 123m + 345m: penchan *or* ryanmen; both readings exist.
        assert_eq!(
            waits("123345m456p789s11z", 2),
            vec![Wait::Ryanmen, Wait::Penchan]
        );
    }

    #[test]
    fn ron_triplet_is_open() {
        let r = readings(&counts_of("456m456p789s11z555z"), &[], 31, true);
        assert!(r[0].sets.contains(&Set::Triplet {
            kind: 31,
            open: true
        }));
        let r = readings(&counts_of("456m456p789s11z555z"), &[], 31, false);
        assert!(r[0].sets.contains(&Set::Triplet {
            kind: 31,
            open: false
        }));
    }

    #[test]
    fn special_shapes() {
        assert!(is_chiitoitsu(&counts_of("1199m2255p3377s44z")));
        assert!(!is_chiitoitsu(&counts_of("1111m2255p3377s44z")));
        assert!(is_kokushi(&counts_of("19m19p19s12345677z")));
        assert!(!is_kokushi(&counts_of("11m19p19s12345677z")));
    }
}

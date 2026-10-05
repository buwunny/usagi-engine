//! Waits and tile acceptance.
//!
//! - **Waits** (*machi*): for a tenpai hand, the kinds that would complete it.
//! - **Tile acceptance** (*ukeire*): for any hand short of a win, the kinds
//!   that would lower its shanten if drawn.
//!
//! Both take a hand of `13 - 3 * melds` concealed tiles, the "waiting for a
//! draw" size.

use crate::counts::Counts;
use crate::kindset::KindSet;

/// The kinds that complete a tenpai hand. Empty if the hand isn't tenpai.
///
/// A kind you already hold all four of is never a wait: there is no fifth
/// copy to draw. (Tenhou's rule for whether such a hand is *tenpai* at an
/// exhaustive draw is a separate question, handled in the engine in M5.)
pub fn waits(counts: &Counts, melds: u8) -> KindSet {
    use crate::shanten::shanten;
    let mut out = KindSet::EMPTY;
    for k in 0..34u8 {
        if counts.get(k) < 4 {
            let mut c = *counts;
            c.add(k);
            if shanten(&c, melds) == -1 {
                out.insert(k);
            }
        }
    }
    out
}

/// The kinds that would lower the hand's shanten if drawn.
///
/// For a tenpai hand this equals [`waits`].
pub fn ukeire(counts: &Counts, melds: u8) -> KindSet {
    use crate::shanten::shanten;
    let base = shanten(counts, melds);
    let mut out = KindSet::EMPTY;
    for k in 0..34u8 {
        if counts.get(k) < 4 {
            let mut c = *counts;
            c.add(k);
            if shanten(&c, melds) < base {
                out.insert(k);
            }
        }
    }
    out
}

/// How many physical tiles are left that would lower shanten, given the
/// kinds the player can see elsewhere (discards, melds, dora indicators).
///
/// For each accepted kind: `4 - held - visible`, never below 0.
pub fn ukeire_count(counts: &Counts, melds: u8, visible: &Counts) -> u32 {
    ukeire(counts, melds)
        .iter()
        .map(|k| 4u32.saturating_sub(counts.get(k) as u32 + visible.get(k) as u32))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::counts_of;

    // cargo test -p usagi-core waits::

    fn kinds(s: &str) -> KindSet {
        counts_of(s).kinds().collect()
    }

    #[test]
    fn simple_waits() {
        // Ryanmen (two-sided) 34m waiting on 2m or 5m.
        assert_eq!(waits(&counts_of("34m456p789s11122z"), 0), kinds("25m"));
        // Shanpon: two pairs, either can become the triplet.
        assert_eq!(waits(&counts_of("123m456p789s1122z"), 0), kinds("12z"));
        // Not tenpai: no waits.
        assert!(waits(&counts_of("13579m13579p135s"), 0).is_empty());
    }

    #[test]
    fn nine_sided_wait() {
        assert_eq!(waits(&counts_of("1112345678999m"), 0), kinds("123456789m"));
    }

    #[test]
    fn thirteen_sided_kokushi() {
        assert_eq!(
            waits(&counts_of("19m19p19s1234567z"), 0),
            kinds("19m19p19s1234567z")
        );
    }

    #[test]
    fn chiitoitsu_wait() {
        assert_eq!(waits(&counts_of("1199m1199p1199s1z"), 0), kinds("1z"));
    }

    #[test]
    fn no_fifth_tile() {
        // 1111m + 234p + 567p + 789s: you hold every 1m, so although the
        // shape "waits" on 1m, there is no 1m left to draw. Only 1m would
        // complete it, so the hand has no waits at all.
        assert!(waits(&counts_of("1111m234p567p789s"), 0).is_empty());
    }

    #[test]
    fn with_melds() {
        assert_eq!(waits(&counts_of("5z"), 4), kinds("5z"));
        assert_eq!(waits(&counts_of("234m55p678s12z"), 1), KindSet::EMPTY);
        assert_eq!(waits(&counts_of("234m55p678s11z"), 1), kinds("5p1z"));
    }

    #[test]
    fn acceptance() {
        // 1-shanten: 34m 67m 456p 789s 11z + a loose 2z. Drawing 2m, 5m or
        // 8m turns a two-sided shape into a set, making it tenpai. Drawing
        // 2z or 1z does not help: there's no room for another shape.
        let hand = counts_of("3467m456p789s112z");
        assert_eq!(ukeire(&hand, 0), kinds("258m"));
        let visible = counts_of("22m");
        // 2m: 4 - 0 held - 2 visible = 2; 5m: 4; 8m: 4.
        assert_eq!(ukeire_count(&hand, 0, &visible), 10);
    }
}

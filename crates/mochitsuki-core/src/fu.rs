//! Fu ("minipoints"). The table is in `docs/reference/scoring.md`.
//!
//! Fu measure how hard the hand's *shape* was: concealed triplets of
//! terminals are worth more than open triplets of simples, awkward waits
//! are worth more than two-sided ones, and so on. Han and fu together give
//! the score. Once a hand reaches mangan (5 han, or 4 han with 40+ fu, or
//! 3 han with 70+ fu) fu stop mattering.

use crate::hand::{Reading, Set, Wait};
use crate::tile::Tile;
use crate::yaku::WinContext;

/// Fu of a standard hand reading, rounded **up** to a multiple of 10.
///
/// Tenhou rules, in order:
/// 1. Pinfu tsumo: exactly 20 (no tsumo fu). Pinfu ron: 30.
/// 2. Otherwise start at 20 ("futei").
/// 3. +10 for a closed hand won by ron (*menzen kafu*).
/// 4. +2 for tsumo.
/// 5. Each triplet/kan, by the table in the reference doc.
/// 6. The pair: +2 for a dragon, +2 for your seat wind, +2 for the round
///    wind (so a double-wind pair is +4 on Tenhou).
/// 7. +2 for a kanchan, penchan or tanki wait.
/// 8. An open hand that would total 20 (all sequences, no extras, ron) is
///    raised to 30.
///
/// Then round up to the next 10.
pub fn fu(reading: &Reading, ctx: &WinContext) -> u8 {
    if crate::yaku::is_pinfu(reading, ctx) {
        return if ctx.tsumo { 20 } else { 30 };
    }
    let mut fu: u32 = 20;
    if reading.closed && !ctx.tsumo {
        fu += 10;
    }
    if ctx.tsumo {
        fu += 2;
    }
    for s in &reading.sets {
        fu += set_fu(s) as u32;
    }
    fu += pair_fu(reading.pair, ctx) as u32;
    if matches!(reading.wait, Wait::Kanchan | Wait::Penchan | Wait::Tanki) {
        fu += 2;
    }
    if !reading.closed && fu == 20 {
        fu = 30;
    }
    (fu.div_ceil(10) * 10) as u8
}

/// Fu for one set (sequences are 0).
pub fn set_fu(set: &Set) -> u8 {
    let (kind, open, kan) = match *set {
        Set::Sequence { .. } => return 0,
        Set::Triplet { kind, open } => (kind, open, false),
        Set::Kan { kind, open } => (kind, open, true),
    };
    let mut f = 2;
    if Tile::from_kind(kind).is_terminal_or_honor() {
        f *= 2;
    }
    if !open {
        f *= 2;
    }
    if kan {
        f *= 4;
    }
    f
}

/// Fu for the pair.
pub fn pair_fu(kind: u8, ctx: &WinContext) -> u8 {
    let mut f = 0;
    if kind >= crate::tile::HAKU {
        f += 2;
    }
    if kind == ctx.seat_wind {
        f += 2;
    }
    if kind == ctx.round_wind {
        f += 2;
    }
    f
}

/// Seven pairs are always exactly 25 fu (not rounded).
pub const CHIITOITSU_FU: u8 = 25;

#[cfg(test)]
mod tests {
    // cargo test -p mochitsuki-core fu::
    // Most fu checks live in tests/scoring_cases.rs, which has a fu column
    // for every case. Add your own small cases here as you go, for example
    // one test per row of the fu table.
}

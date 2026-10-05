//! A fixed numbering of actions, for a network's policy output.
//!
//! | Index | Action |
//! | --- | --- |
//! | 0..37 | `Discard(t)`, by tile code (34..37 are the red fives) |
//! | 37..74 | `Riichi(t)`, by tile code |
//! | 74 | `Tsumo` |
//! | 75 | `Ron` |
//! | 76..82 | `Chi`: 76 + position of the called tile in the run (0 lowest, 1 middle, 2 highest), + 3 if it uses a red five from the hand |
//! | 82..84 | `Pon`: 82, or 83 if it uses a red five from the hand |
//! | 84 | `Daiminkan` |
//! | 85..119 | `Ankan(kind)` |
//! | 119..153 | `Kakan`, by kind |
//! | 153 | `Kyuushu` |
//! | 154 | `Pass` |
//!
//! Every legal action in any state gets a different index, so a mask over
//! [`NUM_ACTIONS`] plus [`action_at`] round-trips exactly.

use usagi_engine::{Action, GameState, Phase, Rules};

pub const NUM_ACTIONS: usize = 155;

const RIICHI: usize = 37;
const TSUMO: usize = 74;
const RON: usize = 75;
const CHI: usize = 76;
const PON: usize = 82;
const DAIMINKAN: usize = 84;
const ANKAN: usize = 85;
const KAKAN: usize = 119;
const KYUUSHU: usize = 153;
const PASS: usize = 154;

/// The index of `action`. Chi needs the tile on offer, taken from the
/// game's call window; outside one, a chi has no index.
pub fn index_of<R: Rules>(g: &GameState<R>, action: Action) -> Option<usize> {
    Some(match action {
        Action::Discard(t) => t.code() as usize,
        Action::Riichi(t) => RIICHI + t.code() as usize,
        Action::Tsumo => TSUMO,
        Action::Ron => RON,
        Action::Chi([a, b]) => {
            let Phase::CallWindow { tile, .. } = g.phase else {
                return None;
            };
            let c = tile.kind();
            let pos = [a.kind(), b.kind()].iter().filter(|&&k| k < c).count();
            CHI + pos + 3 * (a.is_red() || b.is_red()) as usize
        }
        Action::Pon([a, b]) => PON + (a.is_red() || b.is_red()) as usize,
        Action::Daiminkan => DAIMINKAN,
        Action::Ankan(kind) => ANKAN + kind as usize,
        Action::Kakan(t) => KAKAN + t.kind() as usize,
        Action::Kyuushu => KYUUSHU,
        Action::Pass => PASS,
    })
}

/// 1 for every index that is legal for `seat` now.
pub fn legal_mask<R: Rules>(g: &GameState<R>, seat: u8) -> [u8; NUM_ACTIONS] {
    let mut mask = [0u8; NUM_ACTIONS];
    for &a in g.legal_actions(seat).iter() {
        if let Some(i) = index_of(g, a) {
            mask[i] = 1;
        }
    }
    mask
}

/// The legal action of `seat` with index `index`, if there is one.
pub fn action_at<R: Rules>(g: &GameState<R>, seat: u8, index: usize) -> Option<Action> {
    g.legal_actions(seat)
        .iter()
        .copied()
        .find(|&a| index_of(g, a) == Some(index))
}

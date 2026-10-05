//! Where the game is in its turn cycle.
//!
//! ```text
//!            draw (automatic)
//!   ┌─────────────────────────────┐
//!   ▼                             │
//! Turn{seat} ──discard──▶ CallWindow ──all pass──▶ (next seat draws)
//!   │  │                     │  │
//!   │  └─ankan/kakan─┐       │  └─pon/chi──▶ AfterCall{caller} (discard only)
//!   │                ▼       │  └─daiminkan─▶ rinshan draw, Turn{caller}
//!   │          ChankanWindow │
//!   │          (then rinshan └─ron──▶ RoundEnd
//!   │           draw)
//!   └─tsumo / kyuushu / wall empty ──▶ RoundEnd ──▶ start_next_round, or GameEnd
//! ```

use usagi_core::Tile;

use crate::action::Action;

/// Marks "no kind" in [`Phase::AfterCall`]'s `forbid`.
pub const NO_KIND: u8 = u8::MAX;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Phase {
    /// `seat` has just drawn and must act: discard, riichi, tsumo, a kan,
    /// or kyuushu.
    Turn { seat: u8 },
    /// `seat` just called chi or pon and must discard, but not a tile of
    /// the `forbid` kinds (swap-calling, *kuikae*).
    AfterCall { seat: u8, forbid: [u8; 2] },
    /// `from` discarded `tile`. Bit `i` of `pending` is set while seat `i`
    /// still has to answer; `ron` collects who declared ron; `call_seat`
    /// and `call` hold the strongest chi/pon/kan declared so far.
    CallWindow {
        from: u8,
        tile: Tile,
        pending: u8,
        ron: u8,
        call_seat: u8,
        call: Action,
    },
    /// `from` declared a kan with `tile`; others may rob it (chankan).
    /// For an ankan only kokushi may rob.
    ChankanWindow {
        from: u8,
        tile: Tile,
        pending: u8,
        ron: u8,
        ankan: bool,
    },
    /// The hand is over; call `start_next_round` (or
    /// `start_next_round_with_wall`) to deal the next one.
    RoundEnd,
    /// The game is over.
    GameEnd,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_is_small() {
        assert!(std::mem::size_of::<Phase>() <= 12);
    }
}

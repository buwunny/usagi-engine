//! The game state and its 512-byte budget.
//!
//! `GameState` owns everything about a game in progress in one flat block
//! of memory with no pointers, so it is `Copy`: cloning a state for a
//! search or a rollout is one memcpy of 8 cache lines. Observations are
//! derived from it, never the other way round.
//!
//! The compile-time asserts at the bottom fail the build if the layout
//! ever grows past the budget.

use std::marker::PhantomData;

use mochitsuki_core::Tile;
use mochitsuki_core::hand::{Meld, MeldKind};

use crate::phase::Phase;
use crate::rules::{Rules, TenhouRules};
use crate::wall::WALL_SIZE;

/// Most discards one player can make in a hand. 32 is above anything seen
/// in real games; a `debug_assert!` guards it and log replay confirms it.
pub const MAX_DISCARDS: usize = 32;

/// Marks "no riichi" in [`PlayerState::riichi_index`].
pub const NO_RIICHI: u8 = u8::MAX;
/// Marks "no tile" in [`PlayerState::drawn`].
pub const NO_TILE: u8 = u8::MAX;
/// Marks "nobody" in seat-valued fields.
pub const NO_SEAT: u8 = u8::MAX;

/// Bits of a discard byte. The low 6 bits are the tile code (0..=36).
pub mod discard_bits {
    pub const CODE_MASK: u8 = 0b0011_1111;
    /// The discarded tile was the one just drawn (*tsumogiri*).
    pub const TSUMOGIRI: u8 = 0b0100_0000;
    /// Someone called this discard (it moved into their meld).
    pub const CALLED: u8 = 0b1000_0000;
}

/// Bits of [`PlayerState::flags`].
pub mod player_flags {
    pub const RIICHI: u8 = 1 << 0;
    pub const DOUBLE_RIICHI: u8 = 1 << 1;
    /// Ippatsu still possible.
    pub const IPPATSU: u8 = 1 << 2;
    /// Passed on a winning tile this go-around; cleared on the next discard.
    pub const TEMP_FURITEN: u8 = 1 << 3;
    /// Passed on a winning tile while in riichi; lasts the rest of the hand.
    pub const RIICHI_FURITEN: u8 = 1 << 4;
    /// The last draw came from the dead wall (rinshan kaihou).
    pub const RINSHAN: u8 = 1 << 5;
    /// Riichi declared with the last discard, not yet accepted (stick not
    /// paid until the discard passes without a ron).
    pub const RIICHI_PENDING: u8 = 1 << 6;
}

/// A meld packed into two bytes.
///
/// ```text
/// bits  0..=2   kind: 0 chi, 1 pon, 2 daiminkan, 3 kakan, 4 ankan
/// bits  3..=8   first tile kind (0..=33)
/// bits  9..=10  who it was called from, relative: 0 self, 1 right, 2 across, 3 left
/// bit  11       contains a red five
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct PackedMeld(pub u16);

impl PackedMeld {
    pub fn pack(kind: MeldKind, first: u8, from_relative: u8, red: bool) -> PackedMeld {
        let k = match kind {
            MeldKind::Chi => 0,
            MeldKind::Pon => 1,
            MeldKind::Daiminkan => 2,
            MeldKind::Kakan => 3,
            MeldKind::Ankan => 4,
        };
        debug_assert!(first < 34 && from_relative < 4);
        PackedMeld(k | (first as u16) << 3 | (from_relative as u16) << 9 | (red as u16) << 11)
    }

    pub fn kind(self) -> MeldKind {
        match self.0 & 0b111 {
            0 => MeldKind::Chi,
            1 => MeldKind::Pon,
            2 => MeldKind::Daiminkan,
            3 => MeldKind::Kakan,
            _ => MeldKind::Ankan,
        }
    }

    pub fn first(self) -> u8 {
        ((self.0 >> 3) & 0b11_1111) as u8
    }

    pub fn from_relative(self) -> u8 {
        ((self.0 >> 9) & 0b11) as u8
    }

    pub fn has_red(self) -> bool {
        self.0 & (1 << 11) != 0
    }

    /// The same meld with a different kind (pon -> kakan) and red flag.
    pub fn with_kind(self, kind: MeldKind, red: bool) -> PackedMeld {
        PackedMeld::pack(kind, self.first(), self.from_relative(), red)
    }

    pub fn to_meld(self) -> Meld {
        Meld {
            kind: self.kind(),
            first: self.first(),
            reds: self.has_red() as u8,
        }
    }
}

/// One player's private and public state. Exactly 80 bytes.
///
/// `melds` (2-byte alignment) comes first so `#[repr(C)]` needs no padding.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct PlayerState {
    pub melds: [PackedMeld; 4],
    /// Concealed tile counts by kind. Which fives are red is in `reds`.
    pub hand: [u8; 34],
    /// Discards in order: tile code + `discard_bits` flags.
    pub discards: [u8; MAX_DISCARDS],
    pub discard_len: u8,
    pub meld_count: u8,
    /// Index into `discards` of the riichi declaration tile, or `NO_RIICHI`.
    pub riichi_index: u8,
    /// `player_flags` bits.
    pub flags: u8,
    /// Red fives in the concealed hand: bit 0 = 5m, bit 1 = 5p, bit 2 = 5s.
    pub reds: u8,
    /// Code of the tile just drawn (still in `hand`), or `NO_TILE`.
    pub drawn: u8,
}

impl PlayerState {
    pub const EMPTY: PlayerState = PlayerState {
        melds: [PackedMeld(0); 4],
        hand: [0; 34],
        discards: [0; MAX_DISCARDS],
        discard_len: 0,
        meld_count: 0,
        riichi_index: NO_RIICHI,
        flags: 0,
        reds: 0,
        drawn: NO_TILE,
    };

    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    pub fn in_riichi(&self) -> bool {
        self.has(player_flags::RIICHI)
    }

    pub fn melds(&self) -> &[PackedMeld] {
        &self.melds[..self.meld_count as usize]
    }

    pub fn discards(&self) -> &[u8] {
        &self.discards[..self.discard_len as usize]
    }

    /// No meld opens the hand (closed kans are fine).
    pub fn is_closed(&self) -> bool {
        self.melds().iter().all(|m| m.kind() == MeldKind::Ankan)
    }

    pub fn concealed_count(&self) -> u32 {
        self.hand.iter().map(|&c| c as u32).sum()
    }
}

/// Round wind, dealer number and honba.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Round {
    /// 0..=3 East 1-4, 4..=7 South 1-4, 8..=11 West 1-4.
    /// Dealer seat = `index % 4`, round wind = `EAST + index / 4`.
    pub index: u8,
    /// Repeat counter (*honba*).
    pub honba: u8,
}

/// The whole game. At most 512 bytes, 64-byte aligned.
#[repr(C, align(64))]
#[derive(Clone, Copy, Debug)]
pub struct GameState<R: Rules = TenhouRules> {
    /// Tile codes in draw order; see `wall.rs` for the layout.
    pub wall: [Tile; WALL_SIZE],
    pub players: [PlayerState; 4],
    /// Scores in points.
    pub scores: [i32; 4],
    /// Seed for the walls of later hands.
    pub seed: u64,
    /// Next live-wall index to draw.
    pub wall_pos: u8,
    /// Rinshan tiles drawn so far this hand (0..=4).
    pub dead_pos: u8,
    /// Completed kans this hand (0..=4).
    pub kan_count: u8,
    /// Dora indicators face up (1..=5).
    pub dora_revealed: u8,
    /// Open-kan dora indicators waiting to be flipped at the next discard.
    pub pending_dora: u8,
    pub round: Round,
    /// Riichi sticks on the table (1000 points each).
    pub riichi_sticks: u8,
    /// Hands played so far, used to derive each hand's wall seed.
    pub hand_number: u16,
    /// For each seat, the seat liable (*pao*) for its daisangen or
    /// daisuushi, or `NO_SEAT`.
    pub pao: [u8; 4],
    pub phase: Phase,
    pub rules: PhantomData<R>,
}

const _: () = assert!(std::mem::size_of::<PlayerState>() == 80);
const _: () = assert!(std::mem::size_of::<GameState<TenhouRules>>() <= 512);
const _: () = assert!(std::mem::align_of::<GameState<TenhouRules>>() == 64);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget() {
        assert_eq!(std::mem::size_of::<GameState>(), 512);
    }

    #[test]
    fn packed_meld_round_trip() {
        for kind in [
            MeldKind::Chi,
            MeldKind::Pon,
            MeldKind::Daiminkan,
            MeldKind::Kakan,
            MeldKind::Ankan,
        ] {
            for first in [0u8, 13, 33] {
                for from in 0..4 {
                    for red in [false, true] {
                        let m = PackedMeld::pack(kind, first, from, red);
                        assert_eq!(m.kind(), kind);
                        assert_eq!(m.first(), first);
                        assert_eq!(m.from_relative(), from);
                        assert_eq!(m.has_red(), red);
                    }
                }
            }
        }
    }
}

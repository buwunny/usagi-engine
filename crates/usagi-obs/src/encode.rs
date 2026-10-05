//! One seat's view of the game as fixed-size arrays.
//!
//! Everything is relative to the observing seat: "player 0" is the seat
//! itself, 1 the next seat to act (on its right), 2 across, 3 on its left.
//! An [`Observation`] is plain bytes with a fixed layout, so Python can view
//! it as NumPy arrays without copying.
//!
//! # Planes (version 1)
//!
//! Each plane is 34 bytes, one per tile kind, holding 0 or 1. "≥n" planes
//! encode a count c as c thermometer planes: plane k is 1 when c > k.
//!
//! | Planes | Count | Meaning |
//! | --- | --- | --- |
//! | [`plane::HAND`] | 4 | Own concealed tiles, ≥1..≥4 |
//! | [`plane::DRAWN`] | 1 | The tile just drawn |
//! | [`plane::OFFERED`] | 1 | The discard (or kakan tile) others may call or ron now |
//! | [`plane::DORA_INDICATORS`] | 4 | Face-up dora indicators, ≥1..≥4 |
//! | [`plane::VISIBLE`] | 4 | Every tile this seat can see, ≥1..≥4 |
//! | [`plane::player`]`(p)` | 12 each | Per player p, see [`plane::per_player`] |
//!
//! # Scalars
//!
//! Small integers, one byte each, listed in [`scalar`]. Scores are kept
//! separately as `i32` points, relative to the seat.

use usagi_core::tile::NUM_KINDS;
use usagi_core::{Tile, hand::MeldKind};
use usagi_engine::state::{discard_bits, player_flags};
use usagi_engine::wall::{self};
use usagi_engine::{GameState, Phase, Rules};

/// Bumped whenever the layout or meaning of any field changes, so stored
/// datasets can be checked against the encoder that made them.
pub const VERSION: u16 = 1;

pub mod plane {
    pub const HAND: usize = 0;
    pub const DRAWN: usize = 4;
    pub const OFFERED: usize = 5;
    pub const DORA_INDICATORS: usize = 6;
    pub const VISIBLE: usize = 10;
    /// First per-player plane.
    pub const PLAYERS: usize = 14;
    pub const PER_PLAYER: usize = 12;

    /// Offsets within one player's planes.
    pub mod per_player {
        /// Their discards (river), ≥1..≥4, including discards called away.
        pub const DISCARDS: usize = 0;
        /// Kinds they discarded straight after drawing them (tsumogiri).
        pub const TSUMOGIRI: usize = 4;
        /// Their riichi declaration tile.
        pub const RIICHI_TILE: usize = 5;
        /// Their most recent discard.
        pub const LAST_DISCARD: usize = 6;
        /// Their discards that someone called.
        pub const CALLED_AWAY: usize = 7;
        /// Tiles in their open melds and kans, ≥1..≥4.
        pub const MELDS: usize = 8;
    }

    /// First plane of relative player `p` (0 = self).
    pub const fn player(p: usize) -> usize {
        PLAYERS + p * PER_PLAYER
    }

    pub const COUNT: usize = PLAYERS + 4 * PER_PLAYER;
}

pub mod scalar {
    /// 0..=3 East 1-4, 4..=7 South, 8..=11 West.
    pub const ROUND: usize = 0;
    pub const HONBA: usize = 1;
    pub const RIICHI_STICKS: usize = 2;
    pub const TILES_LEFT: usize = 3;
    /// The dealer, relative to this seat.
    pub const DEALER: usize = 4;
    /// This seat's wind, 0 = East.
    pub const SEAT_WIND: usize = 5;
    /// What this seat is asked to do now: see [`super::Decision`].
    pub const DECISION: usize = 6;
    /// Red fives in this seat's concealed hand: bit 0 5m, 1 5p, 2 5s.
    pub const OWN_REDS: usize = 7;
    /// 1 while this seat is furiten (discard, temporary or riichi).
    pub const FURITEN: usize = 8;
    /// Per relative player p: `PLAYER + p * PER_PLAYER + offset`.
    pub const PLAYER: usize = 9;
    pub const PER_PLAYER: usize = 6;

    pub mod per_player {
        /// 0 none, 1 riichi, 2 double riichi.
        pub const RIICHI: usize = 0;
        /// 1 while their ippatsu is still live.
        pub const IPPATSU: usize = 1;
        pub const MELD_COUNT: usize = 2;
        pub const DISCARD_COUNT: usize = 3;
        /// Red fives visible in their melds and river.
        pub const VISIBLE_REDS: usize = 4;
        /// Their rank by current score, 0 = first.
        pub const RANK: usize = 5;
    }

    pub const COUNT: usize = PLAYER + 4 * PER_PLAYER;
}

/// What the observing seat must decide (scalar [`scalar::DECISION`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Decision {
    /// Nothing: someone else is acting, or the hand is over.
    None = 0,
    /// Its own turn after a draw.
    Turn = 1,
    /// Its discard after calling chi or pon.
    AfterCall = 2,
    /// Whether to call or ron someone's discard.
    Call = 3,
    /// Whether to rob a kan (chankan).
    Chankan = 4,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Observation {
    pub planes: [[u8; NUM_KINDS]; plane::COUNT],
    pub scalars: [u8; scalar::COUNT],
    /// Scores in points, relative order (index 0 = this seat).
    pub scores: [i32; 4],
}

impl Default for Observation {
    fn default() -> Self {
        Observation {
            planes: [[0; NUM_KINDS]; plane::COUNT],
            scalars: [0; scalar::COUNT],
            scores: [0; 4],
        }
    }
}

impl Observation {
    /// The planes as one flat byte slice (`plane::COUNT * 34` bytes).
    pub fn plane_bytes(&self) -> &[u8] {
        self.planes.as_flattened()
    }
}

fn thermometer(planes: &mut [[u8; NUM_KINDS]], first: usize, kind: usize, count: u8) {
    for k in 0..(count.min(4) as usize) {
        planes[first + k][kind] = 1;
    }
}

/// Builds `seat`'s observation.
pub fn encode<R: Rules>(g: &GameState<R>, seat: u8) -> Observation {
    let mut obs = Observation::default();
    encode_into(g, seat, &mut obs);
    obs
}

/// Like [`encode`], reusing `obs` (every byte is overwritten).
pub fn encode_into<R: Rules>(g: &GameState<R>, seat: u8, obs: &mut Observation) {
    *obs = Observation::default();
    let planes = &mut obs.planes;
    let sc = &mut obs.scalars;
    let me = &g.players[seat as usize];
    let mut visible = [0u8; NUM_KINDS];

    for (k, &n) in me.hand.iter().enumerate() {
        thermometer(planes, plane::HAND, k, n);
        visible[k] += n;
    }
    if me.drawn != usagi_engine::state::NO_TILE {
        let t = Tile::from_code(me.drawn).expect("valid drawn code");
        planes[plane::DRAWN][t.kind() as usize] = 1;
    }
    match g.phase {
        Phase::CallWindow { tile, .. } | Phase::ChankanWindow { tile, .. } => {
            planes[plane::OFFERED][tile.kind() as usize] = 1;
        }
        _ => {}
    }
    let mut indicators = [0u8; NUM_KINDS];
    for i in 0..g.dora_revealed {
        let k = wall::dora_indicator(&g.wall, i).kind() as usize;
        indicators[k] += 1;
        visible[k] += 1;
    }
    for (k, &n) in indicators.iter().enumerate() {
        thermometer(planes, plane::DORA_INDICATORS, k, n);
    }

    let ranks = g.ranks();
    for rel in 0..4u8 {
        let abs = (seat + rel) as usize % 4;
        let p = &g.players[abs];
        let base = plane::player(rel as usize);
        let mut river = [0u8; NUM_KINDS];
        let mut reds = 0u8;
        for (i, &d) in p.discards().iter().enumerate() {
            let t = Tile::from_code(d & discard_bits::CODE_MASK).expect("valid discard");
            let k = t.kind() as usize;
            reds += t.is_red() as u8;
            river[k] += 1;
            if d & discard_bits::TSUMOGIRI != 0 {
                planes[base + plane::per_player::TSUMOGIRI][k] = 1;
            }
            if d & discard_bits::CALLED != 0 {
                planes[base + plane::per_player::CALLED_AWAY][k] = 1;
            } else {
                visible[k] += 1;
            }
            if i as u8 == p.riichi_index {
                planes[base + plane::per_player::RIICHI_TILE][k] = 1;
            }
            if i + 1 == p.discard_len as usize {
                planes[base + plane::per_player::LAST_DISCARD][k] = 1;
            }
        }
        for (k, &n) in river.iter().enumerate() {
            thermometer(planes, base + plane::per_player::DISCARDS, k, n);
        }
        let mut melded = [0u8; NUM_KINDS];
        for m in p.melds() {
            let first = m.first() as usize;
            match m.kind() {
                MeldKind::Chi => (0..3).for_each(|i| melded[first + i] += 1),
                MeldKind::Pon => melded[first] += 3,
                // A closed kan is shown face up in Tenhou, so it's public too.
                MeldKind::Daiminkan | MeldKind::Kakan | MeldKind::Ankan => melded[first] += 4,
            }
            reds += m.has_red() as u8;
        }
        for (k, &n) in melded.iter().enumerate() {
            thermometer(planes, base + plane::per_player::MELDS, k, n);
            visible[k] += n;
        }

        let s = scalar::PLAYER + rel as usize * scalar::PER_PLAYER;
        sc[s + scalar::per_player::RIICHI] = if p.has(player_flags::DOUBLE_RIICHI) {
            2
        } else {
            p.in_riichi() as u8
        };
        sc[s + scalar::per_player::IPPATSU] = p.has(player_flags::IPPATSU) as u8;
        sc[s + scalar::per_player::MELD_COUNT] = p.meld_count;
        sc[s + scalar::per_player::DISCARD_COUNT] = p.discard_len;
        sc[s + scalar::per_player::VISIBLE_REDS] = reds;
        sc[s + scalar::per_player::RANK] = ranks[abs];
        obs.scores[rel as usize] = g.scores[abs];
    }
    for (k, &n) in visible.iter().enumerate() {
        thermometer(planes, plane::VISIBLE, k, n);
    }

    sc[scalar::ROUND] = g.round.index;
    sc[scalar::HONBA] = g.round.honba;
    sc[scalar::RIICHI_STICKS] = g.riichi_sticks;
    sc[scalar::TILES_LEFT] = g.tiles_left();
    sc[scalar::DEALER] = (g.dealer() + 4 - seat) % 4;
    sc[scalar::SEAT_WIND] = g.seat_wind(seat) - usagi_core::tile::EAST;
    sc[scalar::OWN_REDS] = me.reds;
    sc[scalar::FURITEN] = g.is_furiten(seat) as u8;
    sc[scalar::DECISION] = decision(g, seat) as u8;
}

fn decision<R: Rules>(g: &GameState<R>, seat: u8) -> Decision {
    let mine = g.waiting_on() & (1 << seat) != 0;
    match g.phase {
        _ if !mine => Decision::None,
        Phase::Turn { .. } => Decision::Turn,
        Phase::AfterCall { .. } => Decision::AfterCall,
        Phase::CallWindow { .. } => Decision::Call,
        Phase::ChankanWindow { .. } => Decision::Chankan,
        Phase::RoundEnd | Phase::GameEnd => Decision::None,
    }
}

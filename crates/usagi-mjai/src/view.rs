//! What one seat knows, rebuilt from the Mjai events it is sent.
//!
//! [`View`] follows a game from one seat: its own concealed tiles and
//! melds, every river and meld at the table, riichi declarations, scores,
//! dora indicators, the tiles left in the wall and its own furiten. Bots
//! read it to decide; it never sees more than the seat may see, so it
//! works over a real Mjai connection too.

use usagi_core::hand::{Meld, MeldKind};
use usagi_core::score::score;
use usagi_core::shanten::shanten;
use usagi_core::tile::{EAST, NUM_KINDS, dora_from_indicator};
use usagi_core::waits::waits;
use usagi_core::yaku::{Riichi, WinContext};
use usagi_core::{Counts, KindSet, Tile};

use crate::event::{Event, Pai};

/// One discard in a river.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Discard {
    pub tile: Tile,
    pub tsumogiri: bool,
    /// The sideways riichi tile.
    pub riichi: bool,
    /// Taken by a chi, pon or kan.
    pub called: bool,
}

/// One seat's public information.
#[derive(Clone, Debug, Default)]
pub struct Seat {
    pub river: Vec<Discard>,
    pub melds: Vec<Meld>,
    /// Index in `river` of the riichi tile, once the riichi is accepted.
    pub riichi_at: Option<usize>,
    /// Kinds that can't win against this seat right now: its own
    /// discards, plus (in riichi) everything discarded since.
    pub genbutsu: KindSet,
    pub score: i32,
}

impl Seat {
    pub fn in_riichi(&self) -> bool {
        self.riichi_at.is_some()
    }

    pub fn is_open(&self) -> bool {
        self.melds.iter().any(|m| m.kind.is_open())
    }

    /// Tiles shown in melds of this kind.
    fn meld_count_of(&self, kind: u8) -> u8 {
        self.melds
            .iter()
            .map(|m| match m.kind {
                MeldKind::Chi => (m.first..m.first + 3).contains(&kind) as u8,
                MeldKind::Pon => 3 * (m.first == kind) as u8,
                _ => 4 * (m.first == kind) as u8,
            })
            .sum()
    }
}

#[derive(Clone, Debug, Default)]
pub struct View {
    pub me: u8,
    /// Concealed tiles by kind.
    pub hand: Counts,
    /// Red fives held: bit 0 5m, 1 5p, 2 5s.
    pub reds: u8,
    pub seats: [Seat; 4],
    /// Tiles seen outside this hand: discards, others' melds, dora
    /// indicators.
    pub seen: Counts,
    pub dora_markers: Vec<Tile>,
    pub round_wind: u8,
    /// 1..=4 within the round wind.
    pub kyoku: u8,
    pub honba: u8,
    pub kyotaku: u8,
    pub oya: u8,
    pub tiles_left: i32,
    pub kans: u8,
    pub last_draw: Option<Tile>,
    /// The last draw was a kan replacement.
    pub rinshan: bool,
    /// No discard by us yet and no call by anyone (kyuushu window).
    pub first_turn: bool,
    /// Declared riichi (from our own `reach`, before it is accepted).
    pub riichi: bool,
    pub temp_furiten: bool,
    pub riichi_furiten: bool,
    /// Kinds we may not discard right after our call (kuikae).
    pub forbidden: KindSet,
}

impl View {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn my(&self) -> &Seat {
        &self.seats[self.me as usize]
    }

    pub fn meld_count(&self) -> u8 {
        self.my().melds.len() as u8
    }

    pub fn is_closed(&self) -> bool {
        !self.my().is_open()
    }

    pub fn is_dealer(&self) -> bool {
        self.me == self.oya
    }

    pub fn seat_wind(&self, seat: u8) -> u8 {
        EAST + (seat + 4 - self.oya) % 4
    }

    pub fn is_dora(&self, kind: u8) -> bool {
        self.dora_markers
            .iter()
            .any(|m| dora_from_indicator(m.kind()) == kind)
    }

    pub fn dora_value(&self, kind: u8) -> u8 {
        self.dora_markers
            .iter()
            .filter(|m| dora_from_indicator(m.kind()) == kind)
            .count() as u8
    }

    /// Dragons, and the round and seat winds.
    pub fn is_yakuhai(&self, kind: u8) -> bool {
        kind >= 31 || kind == self.round_wind || kind == self.seat_wind(self.me)
    }

    /// Copies of `kind` we can't draw: seen elsewhere or in our hand or
    /// melds.
    pub fn visible(&self, kind: u8) -> u8 {
        (self.seen.get(kind) + self.hand.get(kind) + self.my().meld_count_of(kind)).min(4)
    }

    /// Tiles we haven't seen anywhere.
    pub fn unseen_total(&self) -> u32 {
        (0..NUM_KINDS as u8)
            .map(|k| 4 - self.visible(k) as u32)
            .sum()
    }

    /// Final round of the game (South 4, or later in an extension).
    pub fn all_last(&self) -> bool {
        self.round_wind > EAST && self.kyoku == 4 || self.round_wind > EAST + 1
    }

    pub fn scores(&self) -> [i32; 4] {
        std::array::from_fn(|s| self.seats[s].score)
    }

    /// Our rank by score (1..=4); ties go to the seat nearer the first
    /// dealer, as at the end of a game.
    pub fn rank(&self, seat: u8) -> u8 {
        let me = self.seats[seat as usize].score;
        1 + (0..4u8)
            .filter(|&s| {
                let o = self.seats[s as usize].score;
                o > me || o == me && s < seat
            })
            .count() as u8
    }

    /// The concealed tiles, with red fives where we hold them.
    pub fn concealed(&self) -> Vec<Tile> {
        let mut out = Vec::with_capacity(14);
        for k in 0..NUM_KINDS as u8 {
            let mut n = self.hand.get(k);
            if n > 0 && is_five(k) && self.reds & red_bit(k) != 0 {
                out.push(red_tile(k));
                n -= 1;
            }
            out.extend((0..n).map(|_| Tile::from_kind(k)));
        }
        out
    }

    /// A held tile of kind `k`, keeping a red five if there's a plain one.
    pub fn physical(&self, k: u8) -> Tile {
        if self.holds_red(k) && self.hand.get(k) == 1 {
            red_tile(k)
        } else {
            Tile::from_kind(k)
        }
    }

    pub fn holds_red(&self, k: u8) -> bool {
        is_five(k) && self.reds & red_bit(k) != 0
    }

    /// Whether drawing `kind` would complete our hand.
    pub fn completes(&self, kind: u8) -> bool {
        if self.hand.get(kind) >= 4 {
            return false;
        }
        let mut c = self.hand;
        c.add(kind);
        shanten(&c, self.meld_count()) == -1
    }

    pub fn furiten(&self) -> bool {
        self.temp_furiten
            || self.riichi_furiten
            || waits(&self.hand, self.meld_count())
                .iter()
                .any(|k| self.discarded(k))
    }

    /// Whether we discarded this kind ourselves.
    pub fn discarded(&self, kind: u8) -> bool {
        self.my().river.iter().any(|d| d.tile.kind() == kind)
    }

    /// The win's score, if winning on `tile` has a yaku.
    pub fn win_score(&self, tile: Tile, tsumo: bool, chankan: bool) -> Option<u32> {
        let mut tiles = self.concealed();
        if !tsumo {
            tiles.push(tile);
        }
        let last = self.tiles_left <= 0;
        let ctx = WinContext {
            win_tile: tile,
            tsumo,
            riichi: if self.riichi {
                Riichi::Riichi
            } else {
                Riichi::None
            },
            ippatsu: false,
            rinshan: tsumo && self.rinshan,
            chankan,
            haitei: tsumo && last && !self.rinshan,
            houtei: !tsumo && last && !chankan,
            tenhou: false,
            chiihou: false,
            seat_wind: self.seat_wind(self.me),
            round_wind: self.round_wind,
            dora_indicators: self.dora_markers.clone(),
            ura_indicators: Vec::new(),
        };
        score(&tiles, &self.my().melds, &ctx).map(|r| r.payment.total())
    }

    fn add(&mut self, t: Tile) {
        self.hand.add(t.kind());
        if t.is_red() {
            self.reds |= red_bit(t.kind());
        }
    }

    fn remove(&mut self, t: Tile) {
        self.hand.remove(t.kind());
        if t.is_red() {
            self.reds &= !red_bit(t.kind());
        }
    }

    fn see(&mut self, p: Pai) {
        if let Some(t) = p.tile() {
            if self.seen.get(t.kind()) < 4 {
                self.seen.add(t.kind());
            }
        }
    }

    fn take_from_river(&mut self, target: u8) {
        if let Some(d) = self.seats[target as usize].river.last_mut() {
            d.called = true;
        }
    }

    /// Follows one event. A discard we can win on marks furiten; callers
    /// that are about to declare the win skip this event instead.
    pub fn update(&mut self, e: &Event) {
        match e {
            Event::StartGame { id, .. } => {
                *self = View {
                    me: id.unwrap_or(0),
                    ..View::default()
                };
            }
            Event::StartKyoku {
                bakaze,
                dora_marker,
                kyoku,
                honba,
                kyotaku,
                oya,
                scores,
                tehais,
            } => {
                let me = self.me;
                *self = View {
                    me,
                    round_wind: bakaze.tile().map_or(EAST, |t| t.kind()),
                    kyoku: *kyoku,
                    honba: *honba,
                    kyotaku: *kyotaku,
                    oya: *oya,
                    tiles_left: 70,
                    first_turn: true,
                    ..View::default()
                };
                for (seat, &score) in self.seats.iter_mut().zip(scores) {
                    seat.score = score;
                }
                for p in &tehais[me as usize] {
                    if let Some(t) = p.tile() {
                        self.add(t);
                    }
                }
                if let Some(t) = dora_marker.tile() {
                    self.dora_markers.push(t);
                }
                self.see(*dora_marker);
            }
            Event::Tsumo { actor, pai } => {
                self.tiles_left -= 1;
                if *actor == self.me {
                    if let Some(t) = pai.tile() {
                        self.add(t);
                        self.last_draw = Some(t);
                    }
                } else {
                    self.rinshan = false;
                }
            }
            Event::Dahai {
                actor,
                pai,
                tsumogiri,
            } => {
                let Some(t) = pai.tile() else { return };
                let k = t.kind();
                let a = *actor as usize;
                self.seats[a].river.push(Discard {
                    tile: t,
                    tsumogiri: *tsumogiri,
                    riichi: false,
                    called: false,
                });
                self.seats[a].genbutsu.insert(k);
                if *actor == self.me {
                    self.remove(t);
                    if !self.riichi {
                        self.temp_furiten = false;
                    }
                    self.rinshan = false;
                    self.first_turn = false;
                    self.forbidden = KindSet::EMPTY;
                    self.last_draw = None;
                } else {
                    self.see(*pai);
                    if self.completes(k) {
                        // Passing a winning tile: furiten until our next
                        // discard, or for the hand once in riichi.
                        self.temp_furiten = true;
                        self.riichi_furiten |= self.riichi;
                    }
                }
                for s in 0..4 {
                    if self.seats[s].in_riichi() {
                        self.seats[s].genbutsu.insert(k);
                    }
                }
            }
            Event::Chi {
                actor,
                target,
                pai,
                consumed,
            }
            | Event::Pon {
                actor,
                target,
                pai,
                consumed,
            }
            | Event::Daiminkan {
                actor,
                target,
                pai,
                consumed,
            } => {
                self.first_turn = false;
                self.take_from_river(*target);
                let mut tiles: Vec<Tile> = consumed.iter().filter_map(|p| p.tile()).collect();
                if *actor == self.me {
                    for &t in &tiles {
                        self.remove(t);
                    }
                } else {
                    for &p in consumed {
                        self.see(p);
                    }
                }
                let Some(called) = pai.tile() else { return };
                tiles.push(called);
                let kind = match e {
                    Event::Chi { .. } => MeldKind::Chi,
                    Event::Pon { .. } => MeldKind::Pon,
                    _ => MeldKind::Daiminkan,
                };
                if kind == MeldKind::Daiminkan {
                    self.kans += 1;
                }
                if tiles.len() == if kind.is_kan() { 4 } else { 3 } {
                    self.seats[*actor as usize]
                        .melds
                        .push(Meld::from_tiles(kind, &tiles));
                }
                if *actor == self.me {
                    self.forbidden = kuikae(kind, called.kind(), &tiles);
                }
            }
            Event::Ankan { actor, consumed } => {
                self.first_turn = false;
                self.kans += 1;
                let tiles: Vec<Tile> = consumed.iter().filter_map(|p| p.tile()).collect();
                if *actor == self.me {
                    for &t in &tiles {
                        self.remove(t);
                    }
                    self.rinshan = true;
                } else {
                    for &p in consumed {
                        self.see(p);
                    }
                }
                if tiles.len() == 4 {
                    self.seats[*actor as usize]
                        .melds
                        .push(Meld::from_tiles(MeldKind::Ankan, &tiles));
                }
            }
            Event::Kakan { actor, pai, .. } => {
                self.first_turn = false;
                self.kans += 1;
                let Some(t) = pai.tile() else { return };
                if *actor == self.me {
                    self.remove(t);
                    self.rinshan = true;
                } else {
                    self.see(*pai);
                    if self.completes(t.kind()) {
                        self.temp_furiten = true;
                        self.riichi_furiten |= self.riichi;
                    }
                }
                let seat = &mut self.seats[*actor as usize];
                if let Some(m) = seat
                    .melds
                    .iter_mut()
                    .find(|m| m.kind == MeldKind::Pon && m.first == t.kind())
                {
                    m.kind = MeldKind::Kakan;
                    m.reds += t.is_red() as u8;
                }
                for s in 0..4 {
                    if self.seats[s].in_riichi() {
                        self.seats[s].genbutsu.insert(t.kind());
                    }
                }
            }
            Event::Dora { dora_marker } => {
                if let Some(t) = dora_marker.tile() {
                    self.dora_markers.push(t);
                }
                self.see(*dora_marker);
            }
            Event::Reach { actor } if *actor == self.me => self.riichi = true,
            Event::ReachAccepted { actor } => {
                let seat = &mut self.seats[*actor as usize];
                seat.score -= 1000;
                seat.riichi_at = Some(seat.river.len().saturating_sub(1));
                if let Some(d) = seat.river.last_mut() {
                    d.riichi = true;
                }
                self.kyotaku += 1;
            }
            Event::Hora {
                deltas: Some(d), ..
            }
            | Event::Ryukyoku {
                deltas: Some(d), ..
            } => {
                for (seat, delta) in self.seats.iter_mut().zip(d) {
                    seat.score += delta;
                }
            }
            _ => {}
        }
    }
}

/// Kinds that may not be discarded right after a call (Tenhou forbids
/// swap calls: the called kind, and for a chi on the end of a sequence the
/// kind at the other end too).
pub fn kuikae(kind: MeldKind, called: u8, tiles: &[Tile]) -> KindSet {
    let mut out = KindSet::EMPTY;
    out.insert(called);
    if kind == MeldKind::Chi {
        let lo = tiles.iter().map(|t| t.kind()).min().unwrap_or(called);
        let n = called % 9;
        if called == lo && n <= 5 {
            out.insert(called + 3);
        } else if called == lo + 2 && n >= 3 {
            out.insert(called - 3);
        }
    }
    out
}

pub(crate) fn is_five(kind: u8) -> bool {
    kind < 27 && kind % 9 == 4
}

fn red_bit(kind: u8) -> u8 {
    1 << (kind / 9)
}

pub(crate) fn red_tile(kind: u8) -> Tile {
    Tile::from_code(34 + kind / 9).unwrap()
}

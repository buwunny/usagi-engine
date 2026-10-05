//! A simple rule-based Mjai bot, as a sparring partner and smoke test.
//!
//! - Wins whenever it can (tsumo, or ron when not furiten and the hand has
//!   a yaku).
//! - Otherwise discards to the lowest shanten, then the most tiles that
//!   would improve the hand, keeping dora when it's a tie.
//! - Declares riichi as soon as it is closed and tenpai.
//! - When another player is in riichi and it is not tenpai, folds: it
//!   discards tiles that are safe against every riichi player if it can
//!   (their own discards and anything discarded since their riichi), then
//!   honors, then terminals.
//! - Never calls chi, pon or kan, and never declares kyuushu.
//!
//! It only ever sees what its seat may see, so it works over the table's
//! masked events or over a real Mjai connection.

use usagi_core::hand::Meld;
use usagi_core::score::score;
use usagi_core::shanten::shanten;
use usagi_core::tile::{EAST, NUM_KINDS, dora_from_indicator};
use usagi_core::waits::{ukeire_count, waits};
use usagi_core::yaku::{Riichi, WinContext};
use usagi_core::{Counts, Tile};

use crate::event::{Event, Pai};
use crate::table::Bot;

#[derive(Clone, Debug, Default)]
pub struct Baseline {
    me: u8,
    hand: Counts,
    /// Red fives held: bit 0 5m, 1 5p, 2 5s.
    reds: u8,
    riichi: bool,
    riichi_discard: Option<Tile>,
    others_riichi: [bool; 4],
    /// Per seat, kinds that are safe against that seat (genbutsu), as
    /// bit masks.
    safe: [u64; 4],
    /// Kinds this seat has discarded (for discard furiten), as a bit mask.
    my_river: u64,
    temp_furiten: bool,
    riichi_furiten: bool,
    /// Tiles seen outside this hand: discards, melds, dora indicators.
    seen: Counts,
    dora_markers: Vec<Tile>,
    round_wind: u8,
    oya: u8,
    score: i32,
    tiles_left: i32,
    last_draw: Option<Tile>,
    rinshan: bool,
}

impl Baseline {
    pub fn new() -> Self {
        Self::default()
    }

    fn concealed(&self) -> Vec<Tile> {
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

    fn completes(&self, kind: u8) -> bool {
        if self.hand.get(kind) >= 4 {
            return false;
        }
        let mut c = self.hand;
        c.add(kind);
        shanten(&c, 0) == -1
    }

    fn furiten(&self) -> bool {
        self.temp_furiten
            || self.riichi_furiten
            || waits(&self.hand, 0)
                .iter()
                .any(|k| self.my_river & 1 << k != 0)
    }

    /// Whether winning on `tile` scores (has a yaku).
    fn can_win(&self, tile: Tile, tsumo: bool) -> bool {
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
            chankan: false,
            haitei: tsumo && last && !self.rinshan,
            houtei: !tsumo && last,
            tenhou: false,
            chiihou: false,
            seat_wind: EAST + (self.me + 4 - self.oya) % 4,
            round_wind: self.round_wind,
            dora_indicators: self.dora_markers.clone(),
            ura_indicators: Vec::new(),
        };
        score(&tiles, &[] as &[Meld], &ctx).is_some()
    }

    fn update(&mut self, e: &Event) {
        match e {
            Event::StartGame { id, .. } => {
                *self = Baseline {
                    me: id.unwrap_or(0),
                    ..Baseline::default()
                };
            }
            Event::StartKyoku {
                bakaze,
                dora_marker,
                oya,
                scores,
                tehais,
                ..
            } => {
                let me = self.me;
                *self = Baseline {
                    me,
                    round_wind: bakaze.tile().map_or(EAST, |t| t.kind()),
                    oya: *oya,
                    score: scores[me as usize],
                    tiles_left: 70,
                    ..Baseline::default()
                };
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
            Event::Dahai { actor, pai, .. } => {
                let Some(t) = pai.tile() else { return };
                let k = t.kind() as usize;
                if *actor == self.me {
                    self.remove(t);
                    self.my_river |= 1 << k;
                    self.safe[self.me as usize] |= 1 << k;
                    if !self.riichi {
                        self.temp_furiten = false;
                    }
                    self.rinshan = false;
                } else {
                    self.see(*pai);
                    self.safe[*actor as usize] |= 1 << k;
                    if self.completes(t.kind()) {
                        // Passing a winning tile: furiten until our next
                        // discard, or for the hand once in riichi.
                        self.temp_furiten = true;
                        self.riichi_furiten |= self.riichi;
                    }
                }
                for s in 0..4 {
                    if self.others_riichi[s] {
                        self.safe[s] |= 1 << k;
                    }
                }
            }
            Event::Chi {
                actor, consumed, ..
            }
            | Event::Pon {
                actor, consumed, ..
            }
            | Event::Daiminkan {
                actor, consumed, ..
            } => {
                for &p in consumed {
                    if *actor == self.me {
                        if let Some(t) = p.tile() {
                            self.remove(t);
                        }
                    } else {
                        self.see(p);
                    }
                }
            }
            Event::Ankan { actor, consumed } => {
                for &p in consumed {
                    if *actor == self.me {
                        if let Some(t) = p.tile() {
                            self.remove(t);
                        }
                    } else {
                        self.see(p);
                    }
                }
                if *actor == self.me {
                    self.rinshan = true;
                }
            }
            Event::Kakan { actor, pai, .. } => {
                if *actor == self.me {
                    if let Some(t) = pai.tile() {
                        self.remove(t);
                    }
                    self.rinshan = true;
                } else {
                    self.see(*pai);
                    if pai.tile().is_some_and(|t| self.completes(t.kind())) {
                        self.temp_furiten = true;
                        self.riichi_furiten |= self.riichi;
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
                if *actor == self.me {
                    self.score -= 1000;
                } else {
                    self.others_riichi[*actor as usize] = true;
                }
            }
            _ => {}
        }
    }

    /// Our turn after a draw.
    fn turn(&mut self) -> Event {
        let me = self.me;
        let drawn = self.last_draw;
        if let Some(t) = drawn {
            if shanten(&self.hand, 0) == -1 && self.can_win(t, true) {
                return Event::Hora {
                    actor: me,
                    target: me,
                    pai: Some(t.into()),
                    deltas: None,
                };
            }
        }
        if self.riichi {
            let t = drawn.expect("riichi turns start with a draw");
            return self.dahai(t);
        }
        let choice = self.choose_discard();
        let mut after = self.hand;
        after.remove(choice.kind());
        let ready = shanten(&after, 0) == 0;
        if ready && self.score >= 1000 && self.tiles_left >= 4 {
            self.riichi_discard = Some(choice);
            return Event::Reach { actor: me };
        }
        self.dahai(choice)
    }

    fn dahai(&self, t: Tile) -> Event {
        Event::Dahai {
            actor: self.me,
            pai: t.into(),
            tsumogiri: self.last_draw == Some(t),
        }
    }

    /// The tile to discard (red fives only when it's the last copy).
    fn choose_discard(&self) -> Tile {
        let after = |k: u8| {
            let mut c = self.hand;
            c.remove(k);
            c
        };
        let held = || (0..NUM_KINDS as u8).filter(|&k| self.hand.get(k) > 0);
        let can_tenpai = held().any(|k| shanten(&after(k), 0) <= 0);
        let fold = self.others_riichi.iter().any(|&r| r) && !can_tenpai;
        let key = |k: u8| {
            let c = after(k);
            let s = shanten(&c, 0) as i32;
            let uke = ukeire_count(&c, 0, &self.seen) as i32;
            if fold {
                let safe_vs = (0..4)
                    .filter(|&p| self.others_riichi[p] && self.safe[p] & 1 << k != 0)
                    .count() as i32;
                (safe_vs, honor_safety(k, &self.seen), -s, uke)
            } else {
                let dora = self
                    .dora_markers
                    .iter()
                    .filter(|m| dora_from_indicator(m.kind()) == k)
                    .count() as i32;
                (-s, uke, -dora, terminal_score(k))
            }
        };
        let k = held()
            .max_by_key(|&k| key(k))
            .expect("a turn always has a tile to discard");
        self.physical(k)
    }

    /// A held tile of kind `k`, keeping a red five if there's a plain one.
    fn physical(&self, k: u8) -> Tile {
        let red = is_five(k) && self.reds & red_bit(k) != 0;
        if red && self.hand.get(k) == 1 {
            red_tile(k)
        } else {
            Tile::from_kind(k)
        }
    }

    /// Someone discarded `tile`: ron if we can, else pass.
    fn call(&self, actor: u8, tile: Tile) -> Event {
        if self.completes(tile.kind()) && !self.furiten() && self.can_win(tile, false) {
            Event::Hora {
                actor: self.me,
                target: actor,
                pai: Some(tile.into()),
                deltas: None,
            }
        } else {
            Event::None
        }
    }
}

impl Bot for Baseline {
    fn react(&mut self, events: &[Event]) -> Result<Event, String> {
        // A discard we'll be asked about must not count as a passed win
        // before we decide, so hold the last event back.
        let (last, rest) = match events.split_last() {
            Some(x) => x,
            None => return Ok(Event::None),
        };
        for e in rest {
            self.update(e);
        }
        let me = self.me;
        let answer = match last {
            Event::Tsumo { actor, .. } if *actor == me => {
                self.update(last);
                self.turn()
            }
            Event::Reach { actor } if *actor == me => {
                self.update(last);
                let t = self
                    .riichi_discard
                    .take()
                    .ok_or("reach without a planned discard")?;
                self.dahai(t)
            }
            Event::Dahai { actor, pai, .. } if *actor != me => {
                let tile = pai.tile().ok_or("dahai of an unknown tile")?;
                let answer = self.call(*actor, tile);
                if answer == Event::None {
                    self.update(last);
                }
                answer
            }
            _ => {
                self.update(last);
                Event::None
            }
        };
        Ok(answer)
    }
}

fn is_five(kind: u8) -> bool {
    kind < 27 && kind % 9 == 4
}

fn red_bit(kind: u8) -> u8 {
    1 << (kind / 9)
}

fn red_tile(kind: u8) -> Tile {
    Tile::from_code(34 + kind / 9).unwrap()
}

/// Higher for tiles that are less useful to keep: honors, then terminals.
fn terminal_score(k: u8) -> i32 {
    if k >= 27 {
        2
    } else if k % 9 == 0 || k % 9 == 8 {
        1
    } else {
        0
    }
}

/// When folding without a safe tile: honors with many copies seen are
/// safest, then other honors, then terminals.
fn honor_safety(k: u8, seen: &Counts) -> i32 {
    if k >= 27 {
        3 + seen.get(k) as i32
    } else if k % 9 == 0 || k % 9 == 8 {
        1
    } else {
        0
    }
}

//! Rule-based bots at three strengths, cheap enough to fill every empty
//! seat on a small server.
//!
//! | Level | Plays like |
//! | --- | --- |
//! | [`Level::Easy`] | a beginner: goes for the lowest shanten but often picks the wrong tile, never defends, calls whenever it can |
//! | [`Level::Normal`] | the baseline bot: tile efficiency, riichi when ready, folds against riichi unless ready, calls only for value honors |
//! | [`Level::Hard`] | Normal, plus wait shapes one draw ahead, dora kept, reads on open hands, safer tiles when pushing, calls for all simples, closed kans and kyuushu |
//!
//! In 3,200 games of one Hard against three Normal, Hard averages rank
//! 2.43 (2.5 is even); one Normal against three Easy averages 1.47. Run
//! `cargo run --release -p usagi-mjai --example duel -- hard normal` to
//! check.
//!
//! A decision costs a few hundred shanten lookups at most (microseconds),
//! with no model weights. [`RuleBot`] keeps only what it needs to explain
//! its last decision; the explanation is built when [`Explain::explain`]
//! is called, by deciding again from the same position with notes on.

use usagi_core::hand::MeldKind;
use usagi_core::shanten::{kokushi, shanten};
use usagi_core::tile::NUM_KINDS;
use usagi_core::waits::{ukeire, waits};
use usagi_core::{Counts, KindSet, Tile};

use serde::{Deserialize, Serialize};

use crate::event::{Event, Pai};
use crate::explain::{
    Candidate, DecisionKind, Explain, Explanation, HandProgress, OpponentRead, Placement, Reason,
    SafetyReason, SiteBot, Stance, TileDanger,
};
use crate::table::Bot;
use crate::view::View;

/// How strong a [`RuleBot`] plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Easy,
    Normal,
    Hard,
}

impl Level {
    pub const ALL: [Level; 3] = [Level::Easy, Level::Normal, Level::Hard];

    pub fn name(self) -> &'static str {
        match self {
            Level::Easy => "easy",
            Level::Normal => "normal",
            Level::Hard => "hard",
        }
    }

    pub fn from_name(s: &str) -> Option<Level> {
        Level::ALL.into_iter().find(|l| l.name() == s)
    }

    fn knobs(self) -> Knobs {
        match self {
            Level::Easy => Knobs {
                slip: 0.45,
                blunder: 0.08,
                defense: Defense::None,
                calls: Calls::Greedy,
                lookahead: false,
            },
            Level::Normal => Knobs {
                slip: 0.0,
                blunder: 0.0,
                defense: Defense::Genbutsu,
                calls: Calls::Yakuhai,
                lookahead: false,
            },
            Level::Hard => Knobs {
                slip: 0.0,
                blunder: 0.0,
                defense: Defense::Read,
                calls: Calls::Yaku,
                lookahead: true,
            },
        }
    }
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Defense {
    None,
    /// Fold against riichi with genbutsu, then honors, then terminals.
    Genbutsu,
    /// Genbutsu folds, plus threats from open hands that look ready,
    /// safer tiles when pushing, and the rest of the table read.
    Read,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Calls {
    /// Anything that lowers shanten, half the time, yaku or not.
    Greedy,
    /// Only pon of a value honor pair.
    Yakuhai,
    /// Value honors, all simples, and anything once a yaku is secured.
    Yaku,
}

#[derive(Clone, Copy, Debug)]
struct Knobs {
    /// Chance of picking a random tile among the lowest-shanten ones.
    slip: f32,
    /// Chance of picking a tile that raises shanten.
    blunder: f32,
    defense: Defense,
    calls: Calls,
    /// Rate one-away hands by the waits they can reach.
    lookahead: bool,
}

/// What we were asked to decide.
#[derive(Clone, Copy, Debug)]
enum Ask {
    Turn,
    Call { from: u8, tile: Tile, chankan: bool },
    AfterCall,
}

/// A small deterministic generator (SplitMix64) for the lower levels'
/// mistakes.
#[derive(Clone, Copy, Debug)]
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn chance(&mut self, p: f32) -> bool {
        p > 0.0 && (self.next() >> 40) as f32 / (1u64 << 24) as f32 <= p
    }

    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[(self.next() % xs.len() as u64) as usize]
    }
}

/// A rule-based Mjai bot. See the module docs for the levels.
#[derive(Clone, Debug)]
pub struct RuleBot {
    level: Level,
    knobs: Knobs,
    view: View,
    rng: Rng,
    /// The discard planned with our `reach`.
    riichi_discard: Option<Tile>,
    /// The position and generator state of the last decision, so it can
    /// be explained exactly.
    last: Option<(View, Rng, Ask)>,
}

impl RuleBot {
    /// A bot at `level`; `seed` drives the lower levels' mistakes.
    pub fn new(level: Level, seed: u64) -> Self {
        let knobs = level.knobs();
        RuleBot {
            level,
            knobs,
            view: View::new(),
            rng: Rng(seed ^ 0x5DEE_CE66_D1CE_5EED),
            riichi_discard: None,
            last: None,
        }
    }

    pub fn level(&self) -> Level {
        self.level
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    fn decide(&mut self, ask: Ask) -> Event {
        self.last = Some((self.view.clone(), self.rng, ask));
        let mut d = Decider {
            v: &self.view,
            k: self.knobs,
            level: self.level,
            rng: &mut self.rng,
            notes: None,
        };
        let (answer, riichi_tile) = d.decide(ask);
        self.riichi_discard = riichi_tile;
        answer
    }
}

impl Bot for RuleBot {
    fn react(&mut self, events: &[Event]) -> Result<Event, String> {
        // A discard we'll be asked about must not count as a passed win
        // before we decide, so hold the last event back. A kan's new dora
        // is shown right after the next discard, so look past it.
        let Some(i) = events
            .iter()
            .rposition(|e| !matches!(e, Event::Dora { .. }))
        else {
            for e in events {
                self.view.update(e);
            }
            return Ok(Event::None);
        };
        let last = &events[i];
        for e in events[..i].iter().chain(&events[i + 1..]) {
            self.view.update(e);
        }
        let me = self.view.me;
        let answer = match last {
            Event::Tsumo { actor, .. } if *actor == me => {
                self.view.update(last);
                self.decide(Ask::Turn)
            }
            Event::Reach { actor } if *actor == me => {
                self.view.update(last);
                let t = self
                    .riichi_discard
                    .take()
                    .ok_or("reach without a planned discard")?;
                Event::Dahai {
                    actor: me,
                    pai: t.into(),
                    tsumogiri: self.view.last_draw == Some(t),
                }
            }
            Event::Chi { actor, .. } | Event::Pon { actor, .. } if *actor == me => {
                self.view.update(last);
                self.decide(Ask::AfterCall)
            }
            Event::Dahai { actor, pai, .. } | Event::Kakan { actor, pai, .. } if *actor != me => {
                let tile = pai.tile().ok_or("discard of an unknown tile")?;
                let chankan = matches!(last, Event::Kakan { .. });
                let answer = self.decide(Ask::Call {
                    from: *actor,
                    tile,
                    chankan,
                });
                if !matches!(answer, Event::Hora { .. }) {
                    self.view.update(last);
                }
                answer
            }
            _ => {
                self.view.update(last);
                Event::None
            }
        };
        Ok(answer)
    }
}

impl Explain for RuleBot {
    fn explain(&self) -> Option<Explanation> {
        let (view, rng, ask) = self.last.clone()?;
        let mut rng = rng;
        let mut d = Decider {
            v: &view,
            k: self.knobs,
            level: self.level,
            rng: &mut rng,
            notes: Some(Notes::default()),
        };
        let (chosen, planned) = d.decide(ask);
        let notes = d.notes.take()?;
        Some(d.explanation(ask, chosen, planned, notes))
    }
}

/// Suggests a move for a person: what a bot at `level` would do in their
/// seat, and why.
///
/// `events` is the seat's own event stream (masked as that seat sees it),
/// ending with the event the person is being asked about. Passing only the
/// `start_game` and the events since the last `start_kyoku` is enough.
/// Costs one decision, so hints can be computed only when asked for.
pub fn suggest(level: Level, events: &[Event]) -> Option<Explanation> {
    let mut bot = RuleBot::new(level, 0);
    bot.react(events).ok()?;
    bot.explain()
}

impl SiteBot for RuleBot {
    fn label(&self) -> String {
        self.level.name().to_string()
    }
}

// ----------------------------------------------------------------------
// Deciding
// ----------------------------------------------------------------------

/// One discard option, scored.
#[derive(Clone, Copy, Debug)]
struct Cand {
    kind: u8,
    shanten: i8,
    /// Shanten counting the yaku plan (open hands going for all simples).
    eff: i8,
    uke: u32,
    /// Expected unseen winning tiles once tenpai.
    waits_after: f32,
    value: f32,
    win: f32,
    deal_in: f32,
    score: f32,
}

/// What a decision noted for its explanation.
#[derive(Default)]
struct Notes {
    cands: Vec<Cand>,
    calls: Vec<(Event, f32, i8, u32)>,
    reasons: Vec<Reason>,
    stance: Option<Stance>,
    threats: Vec<u8>,
}

/// Per-opponent reads, computed once per decision.
struct Reads {
    tenpai: [f32; 4],
    /// Points lost when dealing in to each seat.
    cost: [f32; 4],
}

struct Decider<'a> {
    v: &'a View,
    k: Knobs,
    level: Level,
    rng: &'a mut Rng,
    notes: Option<Notes>,
}

impl Decider<'_> {
    fn note(&mut self, r: Reason) {
        if let Some(n) = self.notes.as_mut() {
            n.reasons.push(r);
        }
    }

    /// The answer, and the discard planned with a `reach`.
    fn decide(&mut self, ask: Ask) -> (Event, Option<Tile>) {
        match ask {
            Ask::Turn => self.turn(),
            Ask::AfterCall => {
                let t = self.discard(self.v.forbidden);
                (self.dahai(t), None)
            }
            Ask::Call {
                from,
                tile,
                chankan,
            } => (self.call(from, tile, chankan), None),
        }
    }

    fn dahai(&self, t: Tile) -> Event {
        Event::Dahai {
            actor: self.v.me,
            pai: t.into(),
            tsumogiri: self.v.last_draw == Some(t),
        }
    }

    fn turn(&mut self) -> (Event, Option<Tile>) {
        let v = self.v;
        let me = v.me;
        if let Some(t) = v.last_draw {
            if shanten(&v.hand, v.meld_count()) == -1 {
                if let Some(points) = v.win_score(t, true, false) {
                    self.note(Reason::Win { points });
                    return (
                        Event::Hora {
                            actor: me,
                            target: me,
                            pai: Some(t.into()),
                            deltas: None,
                        },
                        None,
                    );
                }
            }
        }
        if v.riichi {
            let t = v.last_draw.expect("riichi turns start with a draw");
            return (self.dahai(t), None);
        }
        if self.k.defense == Defense::Read && v.first_turn && self.kyuushu() {
            self.note(Reason::Kyuushu);
            return (
                Event::Ryukyoku {
                    actor: Some(me),
                    deltas: None,
                },
                None,
            );
        }
        if self.k.defense == Defense::Read {
            if let Some(k) = self.ankan() {
                self.note(Reason::Kan);
                return (
                    Event::Ankan {
                        actor: me,
                        consumed: vec![Pai::from(Tile::from_kind(k)); 4]
                            .into_iter()
                            .enumerate()
                            .map(|(i, p)| {
                                if i == 0 && v.holds_red(k) {
                                    crate::view::red_tile(k).into()
                                } else {
                                    p
                                }
                            })
                            .collect(),
                    },
                    None,
                );
            }
        }
        let choice = self.discard(KindSet::EMPTY);
        let mut after = v.hand;
        after.remove(choice.kind());
        let ready = v.is_closed() && shanten(&after, v.meld_count()) == 0;
        if ready && v.my().score >= 1000 && v.tiles_left >= 4 {
            self.note(Reason::Riichi);
            return (Event::Reach { actor: me }, Some(choice));
        }
        (self.dahai(choice), None)
    }

    /// Nine kinds: call it off unless the hand is close to thirteen
    /// orphans.
    fn kyuushu(&self) -> bool {
        let h = &self.v.hand;
        let kinds = [0u8, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33]
            .iter()
            .filter(|&&k| h.get(k) > 0)
            .count();
        kinds >= 9 && kokushi(h) >= 3
    }

    /// A closed kan that doesn't slow the hand.
    fn ankan(&self) -> Option<u8> {
        let v = self.v;
        if v.tiles_left <= 0 || v.kans >= 4 {
            return None;
        }
        let m = v.meld_count();
        let best = (0..NUM_KINDS as u8)
            .filter(|&k| v.hand.get(k) > 0)
            .map(|k| {
                let mut c = v.hand;
                c.remove(k);
                shanten(&c, m)
            })
            .min()?;
        let threat = self.reads().tenpai.iter().cloned().fold(0.0, f32::max);
        (0..NUM_KINDS as u8).find(|&k| {
            if v.hand.get(k) != 4 {
                return false;
            }
            let mut c = v.hand;
            for _ in 0..4 {
                c.remove(k);
            }
            let s = shanten(&c, m + 1);
            s <= best && (threat < 0.5 || s <= 0)
        })
    }

    // ------------------------------------------------------------------
    // Reads
    // ------------------------------------------------------------------

    fn reads(&self) -> Reads {
        let v = self.v;
        let mut tenpai = [0.0f32; 4];
        let mut cost = [0.0f32; 4];
        for s in 0..4u8 {
            if s == v.me {
                continue;
            }
            let seat = &v.seats[s as usize];
            let d = seat.river.len() as f32;
            let open = seat.melds.iter().filter(|m| m.kind.is_open()).count();
            let mut p = if seat.in_riichi() {
                1.0
            } else if open == 0 {
                ((d - 8.0) * 0.02).clamp(0.0, 0.2)
            } else {
                ([0.0, 0.1, 0.25, 0.45, 0.85][open.min(4)] + d * 0.015).min(0.9)
            };
            if !seat.in_riichi() {
                let streak = seat
                    .river
                    .iter()
                    .rev()
                    .take(3)
                    .filter(|d| d.tsumogiri)
                    .count();
                p = (p + 0.04 * streak as f32 * (d / 12.0).min(1.0)).min(0.9);
            }
            tenpai[s as usize] = p;
            let dora_shown: u8 = seat
                .melds
                .iter()
                .map(|m| match m.kind {
                    MeldKind::Chi => (m.first..m.first + 3).map(|k| v.dora_value(k)).sum(),
                    MeldKind::Pon => 3 * v.dora_value(m.first),
                    _ => 4 * v.dora_value(m.first),
                } + m.reds)
                .sum();
            let base = if seat.in_riichi() {
                6000.0
            } else if open > 0 {
                3000.0 + 1500.0 * dora_shown as f32
            } else {
                4500.0
            };
            cost[s as usize] = if s == v.oya { base * 1.5 } else { base };
        }
        Reads { tenpai, cost }
    }

    /// Chance `kind` deals in to `seat` if that seat is tenpai.
    fn danger_vs(&self, seat: u8, kind: u8) -> (f32, SafetyReason) {
        let v = self.v;
        let o = &v.seats[seat as usize];
        if o.genbutsu.contains(kind) {
            return (0.0, SafetyReason::Genbutsu);
        }
        if kind >= 27 {
            let vis = v.visible(kind);
            let value = kind >= 31 || kind == v.round_wind || kind == v.seat_wind(seat);
            let p = match vis {
                4.. => 0.0,
                3 => 0.004,
                2 => 0.025,
                1 => 0.06,
                _ => 0.08,
            } * if value { 1.2 } else { 1.0 };
            let r = if vis >= 2 {
                SafetyReason::Honor
            } else {
                SafetyReason::Plain
            };
            return (p, r);
        }
        let n = kind % 9;
        // Two-sided waits that win on `kind`: with the two tiles below
        // (waiting on kind and kind-3) or the two above (kind and kind+3).
        let mut sides = 0;
        let mut open = 0;
        let mut no_chance = false;
        if n >= 3 {
            sides += 1;
            let suji = o.genbutsu.contains(kind - 3);
            let wall = v.visible(kind - 1) == 4 || v.visible(kind - 2) == 4;
            no_chance |= wall && !suji;
            open += !(suji || wall) as u8;
        }
        if n <= 5 {
            sides += 1;
            let suji = o.genbutsu.contains(kind + 3);
            let wall = v.visible(kind + 1) == 4 || v.visible(kind + 2) == 4;
            no_chance |= wall && !suji;
            open += !(suji || wall) as u8;
        }
        let edge = n.min(8 - n); // 0 for 1/9, 1 for 2/8, ...
        if open == 0 {
            let p = [0.02, 0.03, 0.045, 0.04, 0.04][edge as usize];
            let r = if no_chance {
                SafetyReason::NoChance
            } else {
                SafetyReason::Suji
            };
            return (p, r);
        }
        let p = if sides == 2 && open == 1 {
            0.075
        } else {
            [0.06, 0.085, 0.10, 0.12, 0.12][edge as usize]
        };
        let r = if edge == 0 {
            SafetyReason::Terminal
        } else {
            SafetyReason::Plain
        };
        (p, r)
    }

    /// (deal-in chance, expected points lost, main reason) for `kind`.
    fn danger(&self, reads: &Reads, kind: u8) -> (f32, f32, SafetyReason) {
        let mut safe = 1.0f32;
        let mut loss = 0.0f32;
        let mut reason = SafetyReason::Genbutsu;
        let mut worst = -1.0f32;
        for s in 0..4u8 {
            let t = reads.tenpai[s as usize];
            if s == self.v.me || t <= 0.0 {
                continue;
            }
            let (p, r) = self.danger_vs(s, kind);
            let p = p * if self.v.is_dora(kind) { 1.15 } else { 1.0 };
            safe *= 1.0 - t * p;
            loss += t * p * reads.cost[s as usize];
            if t * p > worst {
                worst = t * p;
                reason = r;
            }
        }
        if worst < 0.0 {
            reason = SafetyReason::Plain;
        }
        (1.0 - safe, loss, reason)
    }

    // ------------------------------------------------------------------
    // The hand
    // ------------------------------------------------------------------

    /// Tiles not in our hand that we can't draw: others' discards and
    /// melds, dora indicators, our own melds.
    fn visible_counts(&self) -> Counts {
        let v = self.v;
        let mut c = Counts::new();
        for k in 0..NUM_KINDS as u8 {
            c.0[k as usize] = v.visible(k) - v.hand.get(k);
        }
        c
    }

    /// Whether an open hand already has a yaku that doesn't depend on its
    /// shape: a value-honor triplet, called or concealed.
    fn yaku_secured(&self, hand: &Counts, melds: &[(MeldKind, u8)]) -> bool {
        let v = self.v;
        melds
            .iter()
            .any(|&(k, first)| k != MeldKind::Chi && v.is_yakuhai(first))
            || (27..34).any(|k| v.is_yakuhai(k) && hand.get(k) >= 3)
    }

    fn my_melds(&self) -> Vec<(MeldKind, u8)> {
        self.v
            .my()
            .melds
            .iter()
            .map(|m| (m.kind, m.first))
            .collect()
    }

    /// Rough points if this 13-tile hand (after the discard) wins.
    fn value(&self, hand: &Counts, melds: &[(MeldKind, u8)], reds: u8) -> f32 {
        let v = self.v;
        let closed = melds.iter().all(|&(k, _)| k == MeldKind::Ankan);
        let mut han = reds as f32;
        let mut yakuhai = 0.0;
        let mut simple = true;
        let mut suits = [false; 4];
        for k in 0..NUM_KINDS as u8 {
            let n = hand.get(k);
            if n == 0 {
                continue;
            }
            han += (n * v.dora_value(k)) as f32;
            simple &= !is_yaochu(k);
            suits[(k / 9) as usize] = true;
            if v.is_yakuhai(k) {
                let double = (k == v.round_wind && k == v.seat_wind(v.me)) as u8 as f32;
                if n >= 3 {
                    yakuhai += 1.0 + double;
                } else if n == 2 {
                    yakuhai += 0.3;
                }
            }
        }
        for &(kind, first) in melds {
            let ks: Vec<u8> = if kind == MeldKind::Chi {
                (first..first + 3).collect()
            } else {
                vec![first]
            };
            let copies = match kind {
                MeldKind::Chi => 1,
                MeldKind::Pon => 3,
                _ => 4,
            };
            for &k in &ks {
                han += (copies * v.dora_value(k)) as f32;
                simple &= !is_yaochu(k);
                suits[(k / 9) as usize] = true;
            }
            if kind != MeldKind::Chi && v.is_yakuhai(first) {
                let double = (first == v.round_wind && first == v.seat_wind(v.me)) as u8 as f32;
                yakuhai += 1.0 + double;
            }
        }
        let mut yaku = yakuhai;
        if simple {
            yaku += 1.0;
        }
        let number_suits = suits[..3].iter().filter(|&&x| x).count();
        if number_suits == 1 {
            yaku += if suits[3] { 2.0 } else { 5.0 } - (!closed) as u8 as f32;
        }
        if closed {
            yaku += 1.4; // riichi, plus ura, tsumo and ippatsu on average
        } else if yaku < 1.0 {
            return 0.0;
        }
        points(han + yaku, v.is_dealer())
    }

    /// Chance to win from `shanten` with `uke` improving tiles now (and
    /// `waits` winning tiles once tenpai), with `draws` draws left.
    fn win_chance(&self, shanten: i8, uke: u32, waits: f32, draws: i32) -> f32 {
        let unseen = self.v.unseen_total().max(1) as f32;
        let tenpai_win = |w: f32, d: i32| -> f32 {
            // Our draw plus about half of three discards, per go-around.
            let q = (2.5 * w / unseen).min(0.6);
            (1.0 - (1.0 - q).powi(d.max(0))) * 0.75
        };
        match shanten {
            s if s < 0 => 1.0,
            0 => tenpai_win(uke as f32, draws),
            _ => {
                // Reach the next shanten on each draw with chance q, then
                // continue with typical acceptance for the levels below.
                let mut p = vec![0.0f32; draws.max(0) as usize + 1];
                for (d, x) in p.iter_mut().enumerate() {
                    *x = tenpai_win(waits, d as i32);
                }
                for level in 1..=shanten {
                    let acc = if level == shanten { uke as f32 } else { 22.0 };
                    let q = (acc / unseen).min(0.9);
                    let prev = p.clone();
                    for d in 0..p.len() {
                        let mut total = 0.0;
                        let mut miss = 1.0;
                        for i in 1..=d {
                            total += miss * q * prev[d - i];
                            miss *= 1.0 - q;
                        }
                        p[d] = total;
                    }
                }
                p[draws.max(0) as usize]
            }
        }
    }

    /// For a one-away hand: the average unseen winning tiles after an
    /// improving draw and the best discard.
    fn average_waits(&self, hand: &Counts, vis: &Counts) -> f32 {
        let m = self.v.meld_count();
        let (mut total, mut weight) = (0.0f32, 0.0f32);
        for u in ukeire(hand, m).iter() {
            let left = 4u8.saturating_sub(hand.get(u) + vis.get(u)) as f32;
            if left == 0.0 {
                continue;
            }
            let mut h = *hand;
            h.add(u);
            let mut best = 0u32;
            for d in 0..NUM_KINDS as u8 {
                if h.get(d) == 0 {
                    continue;
                }
                let mut c = h;
                c.remove(d);
                if shanten(&c, m) == 0 {
                    let w = left_of(&waits(&c, m), &c, vis);
                    best = best.max(w);
                }
            }
            total += left * best as f32;
            weight += left;
        }
        if weight == 0.0 { 0.0 } else { total / weight }
    }

    /// Scores every discard and returns the chosen tile.
    fn discard(&mut self, forbidden: KindSet) -> Tile {
        let v = self.v;
        let m = v.meld_count();
        let reads = self.reads();
        let vis = self.visible_counts();
        let melds = self.my_melds();
        let open = !v.is_closed();
        let tanyao_plan = open && !self.yaku_secured(&v.hand, &melds);
        let draws = (v.tiles_left / 4).max(0);
        let mut cands: Vec<Cand> = Vec::with_capacity(14);
        for k in 0..NUM_KINDS as u8 {
            if v.hand.get(k) == 0 || forbidden.contains(k) {
                continue;
            }
            let mut c = v.hand;
            c.remove(k);
            let s = shanten(&c, m);
            let mut seen = vis;
            seen.0[k as usize] += 1;
            let accept = ukeire(&c, m);
            let (mut eff, mut uke) = (s, left_of(&accept, &c, &seen));
            if tanyao_plan {
                let y = (0..NUM_KINDS as u8)
                    .filter(|&x| is_yaochu(x))
                    .map(|x| c.get(x) as i8)
                    .sum::<i8>();
                eff = s.max(y - 1);
                let simple: KindSet = accept.iter().filter(|&x| !is_yaochu(x)).collect();
                uke = left_of(&simple, &c, &seen);
            }
            let reds = v.reds.count_ones() as u8 - (v.holds_red(k) && v.physical(k).is_red()) as u8
                + v.my().melds.iter().map(|m| m.reds).sum::<u8>();
            let (deal_in, _, _) = self.danger(&reads, k);
            cands.push(Cand {
                kind: k,
                shanten: s,
                eff,
                uke,
                waits_after: 0.0,
                value: self.value(&c, &melds, reds),
                win: 0.0,
                deal_in,
                score: 0.0,
            });
        }
        assert!(!cands.is_empty(), "a turn always has a tile to discard");
        let pick = self.pick(&mut cands, &reads, &vis, draws);
        let chosen = cands[pick];
        if self.notes.is_some() {
            self.note_discard(&cands, pick, &reads);
        }
        if let Some(n) = self.notes.as_mut() {
            n.cands = cands;
        }
        v.physical(chosen.kind)
    }

    /// Picks a discard: build the hand when nobody threatens; otherwise
    /// push or fold, by level.
    fn pick(&mut self, cands: &mut [Cand], reads: &Reads, vis: &Counts, draws: i32) -> usize {
        let v = self.v;
        let best_eff = cands.iter().map(|c| c.eff).min().unwrap();
        if self.rng.chance(self.k.blunder) {
            let worse: Vec<usize> = (0..cands.len())
                .filter(|&i| cands[i].eff == best_eff + 1)
                .collect();
            if !worse.is_empty() {
                self.note(Reason::Slip);
                return self.rng.pick(&worse);
            }
        }
        if self.rng.chance(self.k.slip) {
            let same: Vec<usize> = (0..cands.len())
                .filter(|&i| cands[i].eff == best_eff)
                .collect();
            self.note(Reason::Slip);
            return self.rng.pick(&same);
        }
        // Win chance and value, for the push choice and explanations.
        let mut race = 1.0f32;
        for s in 0..4 {
            if s != v.me as usize && v.seats[s].in_riichi() {
                race *= 0.75;
            }
        }
        for c in cands.iter_mut() {
            let waits_after = if c.eff == 0 {
                c.uke as f32
            } else if self.k.lookahead && c.eff == 1 {
                let mut h = v.hand;
                h.remove(c.kind);
                let mut seen = *vis;
                seen.0[c.kind as usize] += 1;
                self.average_waits(&h, &seen)
            } else {
                5.0
            };
            c.waits_after = waits_after;
            c.win = self.win_chance(c.eff, c.uke, waits_after, draws) * race;
        }
        // Who threatens: riichi, and (reading the table) open hands that
        // look ready.
        let threats: Vec<u8> = (0..4u8)
            .filter(|&s| {
                s != v.me
                    && (v.seats[s as usize].in_riichi()
                        || self.k.defense == Defense::Read && reads.tenpai[s as usize] >= 0.6)
            })
            .collect();
        // Ready hands push on; the rest fold.
        let push = threats.is_empty() || best_eff <= 0;
        let defend = !threats.is_empty() && self.k.defense != Defense::None;
        for c in cands.iter_mut() {
            let k = c.kind;
            let dora =
                v.dora_value(k) as f32 + (v.holds_red(k) && v.physical(k).is_red()) as u8 as f32;
            let build = -(c.eff as f32) * 10000.0
                + c.uke as f32 * 10.0
                + if self.k.lookahead && c.eff == 1 {
                    c.waits_after * 12.0
                } else {
                    0.0
                }
                - dora
                    * if self.k.defense == Defense::Read {
                        25.0
                    } else {
                        3.0
                    }
                + terminal_score(k) as f32;
            c.score = if !defend {
                build
            } else if push {
                build
                    - c.deal_in
                        * if self.k.defense == Defense::Read {
                            600.0
                        } else {
                            0.0
                        }
            } else {
                let safe_vs = threats
                    .iter()
                    .filter(|&&p| v.seats[p as usize].genbutsu.contains(k))
                    .count() as f32;
                safe_vs * 1000.0 + honor_safety(k, v) as f32 * 100.0 - c.eff as f32 * 10.0
                    + c.uke as f32 * 0.01
            };
        }
        let stance = if threats.is_empty() {
            Stance::Build
        } else if push {
            Stance::Push
        } else {
            Stance::Fold
        };
        if let Some(n) = self.notes.as_mut() {
            n.stance = Some(stance);
            n.threats = threats;
        }
        best_index(cands)
    }

    fn note_discard(&mut self, cands: &[Cand], pick: usize, reads: &Reads) {
        let v = self.v;
        let c = cands[pick];
        let (_, _, safety) = self.danger(reads, c.kind);
        let (stance, threats) = match self.notes.as_ref() {
            Some(n) => (n.stance.unwrap_or(Stance::Build), n.threats.clone()),
            None => return,
        };
        match stance {
            Stance::Fold => self.note(Reason::Fold { against: threats }),
            Stance::Push if self.k.defense == Defense::Read => self.note(Reason::Push {
                win_chance: c.win,
                value: c.value as u32,
            }),
            _ => {}
        }
        if stance != Stance::Build {
            self.note(Reason::Safe { reason: safety });
        }
        if c.shanten == 0 {
            let mut h = v.hand;
            h.remove(c.kind);
            let ws = waits(&h, v.meld_count())
                .iter()
                .map(|k| Pai::from(Tile::from_kind(k)))
                .collect();
            self.note(Reason::Tenpai { waits: ws });
        }
        let same: Vec<&Cand> = cands.iter().filter(|x| x.eff == c.eff).collect();
        let max_uke = same.iter().map(|x| x.uke).max().unwrap_or(0);
        if stance == Stance::Build || stance == Stance::Push {
            if c.uke == max_uke && c.shanten > 0 {
                self.note(Reason::MostAcceptance { ukeire: c.uke });
            } else if self.k.lookahead && c.eff == 1 {
                self.note(Reason::BetterShape);
            }
            if same
                .iter()
                .any(|x| x.uke >= c.uke && x.kind != c.kind && v.is_dora(x.kind))
            {
                self.note(Reason::KeepDora);
            }
            if same
                .iter()
                .any(|x| x.uke >= c.uke && x.kind != c.kind && v.is_yakuhai(x.kind))
                && !v.is_closed()
            {
                self.note(Reason::KeepYaku);
            }
        }
    }

    // ------------------------------------------------------------------
    // Calls
    // ------------------------------------------------------------------

    fn call(&mut self, from: u8, tile: Tile, chankan: bool) -> Event {
        let v = self.v;
        let me = v.me;
        let k = tile.kind();
        if v.completes(k) && !v.furiten() {
            if let Some(points) = v.win_score(tile, false, chankan) {
                self.note(Reason::Win { points });
                return Event::Hora {
                    actor: me,
                    target: from,
                    pai: Some(tile.into()),
                    deltas: None,
                };
            }
        }
        if chankan || v.riichi || v.tiles_left <= 0 {
            return Event::None;
        }
        let m = v.meld_count();
        let now = shanten(&v.hand, m);
        let melds = self.my_melds();
        let reads = self.reads();
        let threat = (0..4)
            .filter(|&s| s != me as usize)
            .map(|s| reads.tenpai[s])
            .fold(0.0, f32::max);
        // (event, consumed kinds, meld kind)
        let mut options: Vec<(Event, MeldKind, [u8; 2])> = Vec::new();
        if v.hand.get(k) >= 2 {
            let plain = v.hand.get(k) - v.holds_red(k) as u8;
            let consumed: Vec<Pai> = if plain >= 2 {
                vec![Tile::from_kind(k).into(); 2]
            } else {
                vec![crate::view::red_tile(k).into(), Tile::from_kind(k).into()]
            };
            options.push((
                Event::Pon {
                    actor: me,
                    target: from,
                    pai: tile.into(),
                    consumed,
                },
                MeldKind::Pon,
                [k, k],
            ));
        }
        if from == (me + 3) % 4 && k < 27 {
            let n = k % 9;
            let mut shapes = Vec::new();
            if n >= 2 {
                shapes.push([k - 2, k - 1]);
            }
            if (1..=7).contains(&n) {
                shapes.push([k - 1, k + 1]);
            }
            if n <= 6 {
                shapes.push([k + 1, k + 2]);
            }
            for [a, b] in shapes {
                if v.hand.get(a) > 0 && v.hand.get(b) > 0 {
                    options.push((
                        Event::Chi {
                            actor: me,
                            target: from,
                            pai: tile.into(),
                            consumed: vec![v.physical(a).into(), v.physical(b).into()],
                        },
                        MeldKind::Chi,
                        [a, b],
                    ));
                }
            }
        }
        let mut best: Option<(Event, (i8, u32), Reason)> = None;
        for (event, kind, used) in options {
            let mut c = v.hand;
            c.remove(used[0]);
            c.remove(used[1]);
            let tiles: Vec<Tile> = used
                .iter()
                .map(|&x| Tile::from_kind(x))
                .chain([tile])
                .collect();
            let forbid = crate::view::kuikae(kind, k, &tiles);
            let first = if kind == MeldKind::Chi {
                used[0].min(k)
            } else {
                k
            };
            let mut new_melds = melds.clone();
            new_melds.push((kind, first));
            // Best discard after the call.
            let mut after: Option<(i8, i8, u32)> = None;
            let vis = self.visible_counts();
            for d in 0..NUM_KINDS as u8 {
                if c.get(d) == 0 || forbid.contains(d) {
                    continue;
                }
                let mut h = c;
                h.remove(d);
                let s = shanten(&h, m + 1);
                let y = (0..NUM_KINDS as u8)
                    .filter(|&x| is_yaochu(x))
                    .map(|x| h.get(x) as i8)
                    .sum::<i8>();
                let u = left_of(&ukeire(&h, m + 1), &h, &vis);
                if after.is_none_or(|(bs, _, bu)| (s, -(u as i32)) < (bs, -(bu as i32))) {
                    after = Some((s, y, u));
                }
            }
            let Some((s, yaochu_left, u)) = after else {
                continue;
            };
            let yakuhai = kind == MeldKind::Pon && v.is_yakuhai(k);
            let secured = yakuhai || self.yaku_secured(&c, &new_melds);
            let simple_melds = new_melds.iter().all(|&(mk, f)| {
                if mk == MeldKind::Chi {
                    !is_yaochu(f) && !is_yaochu(f + 2)
                } else {
                    !is_yaochu(f)
                }
            });
            let tanyao = simple_melds && yaochu_left <= 1;
            let take = match self.k.calls {
                Calls::Greedy => yakuhai && s <= now || s < now && self.rng.chance(0.5),
                Calls::Yakuhai => yakuhai && s <= now,
                Calls::Yaku => {
                    let calm = threat < 0.5 || s <= 0;
                    let keep_closed = v.is_closed() && now <= 1 && !yakuhai;
                    calm && (yakuhai && s <= now || (secured || tanyao) && s < now && !keep_closed)
                }
            };
            if !take {
                continue;
            }
            let reason = if yakuhai {
                Reason::YakuhaiCall
            } else if tanyao && !secured {
                Reason::TanyaoCall
            } else {
                Reason::MostAcceptance { ukeire: u }
            };
            if let Some(n) = self.notes.as_mut() {
                n.calls.push((event.clone(), -(s as f32), s, u));
            }
            if best
                .as_ref()
                .is_none_or(|(_, (bs, bu), _)| (s, -(u as i32)) < (*bs, -(*bu as i32)))
            {
                best = Some((event, (s, u), reason));
            }
        }
        match best {
            Some((event, _, reason)) => {
                self.note(reason);
                event
            }
            None => {
                if v.hand.get(k) >= 2 || from == (me + 3) % 4 {
                    self.note(Reason::NoCall);
                }
                Event::None
            }
        }
    }

    // ------------------------------------------------------------------
    // Explaining
    // ------------------------------------------------------------------

    fn explanation(
        &mut self,
        ask: Ask,
        chosen: Event,
        planned: Option<Tile>,
        notes: Notes,
    ) -> Explanation {
        let v = self.v;
        let reads = self.reads();
        let m = v.meld_count();
        // The hand after the move.
        let mut hand = v.hand;
        let mut melds = m;
        match &chosen {
            Event::Dahai { pai, .. } => {
                if let Some(t) = pai.tile() {
                    hand.remove(t.kind());
                }
            }
            Event::Reach { .. } => {
                if let Some(t) = planned {
                    hand.remove(t.kind());
                }
            }
            Event::Chi { consumed, .. } | Event::Pon { consumed, .. } => {
                for p in consumed {
                    if let Some(t) = p.tile() {
                        hand.remove(t.kind());
                    }
                }
                melds += 1;
            }
            Event::Ankan { consumed, .. } => {
                if let Some(t) = consumed.first().and_then(|p| p.tile()) {
                    for _ in 0..4 {
                        hand.remove(t.kind());
                    }
                }
                melds += 1;
            }
            _ => {}
        }
        let vis = self.visible_counts();
        let s = if hand.total() % 3 == 1 {
            shanten(&hand, melds)
        } else {
            (0..NUM_KINDS as u8)
                .filter(|&k| hand.get(k) > 0)
                .map(|k| {
                    let mut c = hand;
                    c.remove(k);
                    shanten(&c, melds)
                })
                .min()
                .unwrap_or(8)
        };
        let (uke, ws) = if hand.total() % 3 == 1 {
            let u = left_of(&ukeire(&hand, melds), &hand, &vis);
            let w = if s == 0 {
                waits(&hand, melds)
                    .iter()
                    .map(|k| Pai::from(Tile::from_kind(k)))
                    .collect()
            } else {
                Vec::new()
            };
            (u, w)
        } else {
            (0, Vec::new())
        };
        let discarded = match &chosen {
            Event::Dahai { pai, .. } => pai.tile(),
            Event::Reach { .. } => planned,
            _ => None,
        };
        let picked =
            discarded.and_then(|t| notes.cands.iter().find(|c| c.kind == t.kind()).copied());
        let draws = (v.tiles_left / 4).max(0);
        let win_chance = match picked {
            Some(c) if c.win > 0.0 => c.win,
            _ if matches!(chosen, Event::Hora { .. }) => 1.0,
            _ => self.win_chance(s, uke, 5.0, draws),
        };
        let value = picked.map_or(0, |c| c.value as u32);
        let mut danger: Vec<TileDanger> = (0..NUM_KINDS as u8)
            .filter(|&k| v.hand.get(k) > 0)
            .map(|k| {
                let (p, _, r) = self.danger(&reads, k);
                TileDanger {
                    pai: v.physical(k).into(),
                    deal_in: round3(p),
                    reason: r,
                }
            })
            .collect();
        danger.sort_by(|a, b| a.deal_in.total_cmp(&b.deal_in));
        let mut cands: Vec<Candidate> = notes
            .cands
            .iter()
            .map(|c| Candidate {
                action: Event::Dahai {
                    actor: v.me,
                    pai: v.physical(c.kind).into(),
                    tsumogiri: v.last_draw.is_some_and(|t| t.kind() == c.kind),
                },
                score: round3(c.score),
                shanten: c.eff,
                ukeire: c.uke,
                deal_in: round3(c.deal_in),
                win_chance: round3(c.win),
                value: c.value as u32,
            })
            .chain(notes.calls.iter().map(|(e, sc, s, u)| Candidate {
                action: e.clone(),
                score: *sc,
                shanten: *s,
                ukeire: *u,
                deal_in: 0.0,
                win_chance: 0.0,
                value: 0,
            }))
            .collect();
        cands.sort_by(|a, b| b.score.total_cmp(&a.score));
        let threat = reads.tenpai.iter().cloned().fold(0.0, f32::max);
        let stance = notes.stance.unwrap_or(if threat >= 0.5 {
            Stance::Balance
        } else {
            Stance::Build
        });
        Explanation {
            bot: format!("rule bot ({})", self.level),
            seat: v.me,
            kind: match ask {
                Ask::Turn => DecisionKind::Turn,
                Ask::Call { .. } => DecisionKind::Call,
                Ask::AfterCall => DecisionKind::AfterCall,
            },
            chosen,
            riichi_discard: planned.map(Pai::from),
            stance,
            hand: HandProgress {
                shanten: s,
                ukeire: uke,
                waits: ws,
                value,
            },
            opponents: (0..4u8)
                .filter(|&s| s != v.me)
                .map(|s| OpponentRead {
                    seat: s,
                    tenpai: round3(reads.tenpai[s as usize]),
                    riichi: v.seats[s as usize].in_riichi(),
                    melds: v.seats[s as usize].melds.len() as u8,
                })
                .collect(),
            danger,
            win_chance: round3(win_chance),
            placement: Placement {
                rank: v.rank(v.me),
                scores: v.scores(),
                all_last: v.all_last(),
                predicted: None,
            },
            candidates: cands,
            reasons: notes.reasons,
        }
    }
}

fn best_index(cands: &[Cand]) -> usize {
    let mut best = 0;
    for i in 1..cands.len() {
        if cands[i].score > cands[best].score {
            best = i;
        }
    }
    best
}

/// Unseen copies of the kinds in `set`.
fn left_of(set: &KindSet, hand: &Counts, vis: &Counts) -> u32 {
    set.iter()
        .map(|k| 4u32.saturating_sub(hand.get(k) as u32 + vis.get(k) as u32))
        .sum()
}

fn is_yaochu(k: u8) -> bool {
    k >= 27 || k % 9 == 0 || k % 9 == 8
}

/// Rough payment for `han` (30 fu), interpolating between whole han.
fn points(han: f32, dealer: bool) -> f32 {
    const TABLE: [f32; 14] = [
        0.0, 1000.0, 2000.0, 3900.0, 7700.0, 8000.0, 12000.0, 12000.0, 16000.0, 16000.0, 16000.0,
        24000.0, 24000.0, 32000.0,
    ];
    let h = han.clamp(0.0, 13.0);
    let lo = h.floor() as usize;
    let hi = (lo + 1).min(13);
    let p = TABLE[lo] + (TABLE[hi] - TABLE[lo]) * (h - lo as f32);
    if dealer { p * 1.5 } else { p }
}

fn round3(x: f32) -> f32 {
    (x * 1000.0).round() / 1000.0
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
fn honor_safety(k: u8, v: &View) -> i32 {
    if k >= 27 {
        3 + v.seen.get(k) as i32
    } else if k % 9 == 0 || k % 9 == 8 {
        1
    } else {
        0
    }
}

//! Replays logged games through the engine.
//!
//! Each hand is rebuilt from its log: the wall is reconstructed from the
//! starting hands, every draw, the dora indicators and the ura indicators
//! (tiles nobody ever saw don't affect anything), the engine is dealt that
//! wall at the logged round, honba, sticks and scores, and then every
//! logged decision is fed to [`GameState::step`]. Passing on a call is not
//! logged, so the replayer passes for every seat that didn't act.
//!
//! A hand matches when every action was legal, every draw was the logged
//! tile, and the wins and score changes equal the log's. Hands are
//! replayed independently, so one mismatch doesn't hide the rest; on top
//! of that, in hanchan logs the state after each hand must equal the next
//! hand's start (round, honba, sticks, scores), and the last hand must end
//! the game with the logged final scores.

use std::fmt;

use mochitsuki_core::Tile;
use mochitsuki_engine::wall::{DORA_START, RINSHAN_START, URA_START, WALL_SIZE};
use mochitsuki_engine::{Action, Event, GameState, Phase, Round, TenhouRules};

use crate::mjlog::{Agari, CallKind, DrawKind, Game, Init, LogEvent, TileId, tile_of};

/// The first thing that went wrong in one hand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    /// Hand number within the log, from 0.
    pub hand: usize,
    pub round: u8,
    pub honba: u8,
    /// Index of the event within the hand (0 is the `INIT`).
    pub event: usize,
    pub message: String,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const WINDS: [&str; 3] = ["East", "South", "West"];
        write!(
            f,
            "hand {} ({} {}-{}), event {}: {}",
            self.hand,
            WINDS.get(self.round as usize / 4).unwrap_or(&"?"),
            self.round % 4 + 1,
            self.honba,
            self.event,
            self.message
        )
    }
}

/// What replaying one log found.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub hands: usize,
    /// One entry per hand that didn't match, plus game-flow mismatches
    /// (reported against the hand that should have produced the state).
    pub mismatches: Vec<Mismatch>,
    /// The log stops partway through its last hand (an excerpt); that
    /// hand was checked up to where it stops.
    pub truncated: bool,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.mismatches.is_empty()
    }
}

type State = GameState<TenhouRules>;

/// Replays every hand of `game`.
pub fn replay(game: &Game) -> Report {
    let mut report = Report::default();
    if !game.is_tenhou_ranked_rules() {
        report.mismatches.push(Mismatch {
            hand: 0,
            round: 0,
            honba: 0,
            event: 0,
            message: format!(
                "room type {:#x} is not Tenhou four-player ranked rules",
                game.room
            ),
        });
        return report;
    }
    let hands = game.hands();
    report.hands = hands.len();
    for (h, events) in hands.iter().enumerate() {
        let LogEvent::Init(init) = &events[0] else {
            unreachable!("hands() starts every hand with Init")
        };
        let at = |event: usize, message: String| Mismatch {
            hand: h,
            round: init.round,
            honba: init.honba,
            event,
            message,
        };
        match replay_hand(init, &events[1..], game.red_fives()) {
            Err((event, message)) => report.mismatches.push(at(event + 1, message)),
            Ok(None) if h + 1 == hands.len() => report.truncated = true,
            Ok(None) => report
                .mismatches
                .push(at(events.len(), "hand ends without a result".into())),
            Ok(Some(state)) if game.hanchan() => {
                let next = hands.get(h + 1).map(|e| match &e[0] {
                    LogEvent::Init(i) => i,
                    _ => unreachable!(),
                });
                let last = events.len();
                if let Err(message) = check_flow(&state, next, final_scores(events)) {
                    report.mismatches.push(at(last, message));
                }
            }
            Ok(_) => {}
        }
    }
    report
}

fn final_scores(events: &[LogEvent]) -> Option<[i32; 4]> {
    events.iter().rev().find_map(|e| match e {
        LogEvent::Agari(a) => a.owari,
        LogEvent::Ryuukyoku(r) => r.owari,
        _ => None,
    })
}

/// After a hand, the engine's next state must be the log's next `INIT`, or
/// the end of the game with the logged final scores.
fn check_flow(g: &State, next: Option<&Init>, owari: Option<[i32; 4]>) -> Result<(), String> {
    match (next, owari) {
        (Some(n), _) => {
            if g.phase == Phase::GameEnd {
                return Err("engine ended the game but the log goes on".into());
            }
            let want = (n.round, n.honba, n.sticks, n.scores);
            let got = (g.round.index, g.round.honba, g.riichi_sticks, g.scores);
            if want != got {
                return Err(format!(
                    "next hand: log (round, honba, sticks, scores) {want:?}, engine {got:?}"
                ));
            }
        }
        (None, Some(fin)) => {
            if g.phase != Phase::GameEnd {
                return Err("log ended the game but the engine goes on".into());
            }
            if g.scores != fin {
                return Err(format!("final scores: log {fin:?}, engine {:?}", g.scores));
            }
        }
        // A log cut off mid-game: nothing to compare.
        (None, None) => {}
    }
    Ok(())
}

/// Rebuilds the hand's wall in the engine's layout from what the log shows.
pub fn build_wall(
    init: &Init,
    events: &[LogEvent],
    red: bool,
) -> Result<[Tile; WALL_SIZE], (usize, String)> {
    let mut slots: [Option<TileId>; WALL_SIZE] = [None; WALL_SIZE];
    let mut used = [false; 136];
    let mut put = |slot: usize, id: TileId, ev: usize| -> Result<(), (usize, String)> {
        if used[id as usize] {
            return Err((ev, format!("tile id {id} appears twice")));
        }
        if slots[slot].is_some() {
            return Err((ev, format!("wall slot {slot} filled twice")));
        }
        used[id as usize] = true;
        slots[slot] = Some(id);
        Ok(())
    };
    for i in 0..4u8 {
        let seat = (init.dealer + i) % 4;
        for (k, &id) in init.hands[seat as usize].iter().enumerate() {
            put(i as usize * 13 + k, id, 0)?;
        }
    }
    put(DORA_START as usize, init.dora_indicator, 0)?;
    let (mut live, mut rinshan, mut dora) = (52usize, RINSHAN_START as usize, 1usize);
    let mut after_kan = false;
    let mut ura_done = false;
    for (i, e) in events.iter().enumerate() {
        let ev = i + 1;
        match e {
            LogEvent::Draw { tile, .. } => {
                if after_kan {
                    if rinshan >= DORA_START as usize {
                        return Err((ev, "more than four rinshan draws".into()));
                    }
                    put(rinshan, *tile, ev)?;
                    rinshan += 1;
                } else {
                    let kans = rinshan - RINSHAN_START as usize;
                    if live >= RINSHAN_START as usize - kans {
                        return Err((ev, "draw after the live wall ran out".into()));
                    }
                    put(live, *tile, ev)?;
                    live += 1;
                }
                after_kan = false;
            }
            LogEvent::Call { call, .. } => {
                after_kan = matches!(
                    call.kind,
                    CallKind::Ankan | CallKind::Kakan | CallKind::Daiminkan
                );
            }
            LogEvent::Dora { indicator } => {
                if dora >= 5 {
                    return Err((ev, "more than five dora indicators".into()));
                }
                put(DORA_START as usize + dora, *indicator, ev)?;
                dora += 1;
            }
            LogEvent::Agari(a) if !ura_done && !a.ura_indicators.is_empty() => {
                for (k, &id) in a.ura_indicators.iter().enumerate() {
                    put(URA_START as usize + k, id, ev)?;
                }
                ura_done = true;
            }
            _ => {}
        }
    }
    let mut spare = (0..136u8).filter(|&id| !used[id as usize]);
    let mut wall = [Tile::from_kind(0); WALL_SIZE];
    for (w, s) in wall.iter_mut().zip(slots) {
        let id = s.unwrap_or_else(|| spare.next().expect("136 slots, 136 ids"));
        *w = tile_of(id, red);
    }
    Ok(wall)
}

/// Plays one hand; `events` excludes the `INIT`. On failure, returns the
/// index into `events` and what went wrong.
/// `Ok(None)` means the log stopped before the hand ended.
fn replay_hand(
    init: &Init,
    events: &[LogEvent],
    red: bool,
) -> Result<Option<State>, (usize, String)> {
    let wall = build_wall(init, events, red).map_err(|(i, m)| (i.saturating_sub(1), m))?;
    let round = Round {
        index: init.round,
        honba: init.honba,
    };
    let mut g = State::with_hand(round, init.sticks, init.scores, wall);
    let mut out: Vec<Event> = Vec::new();
    let mut riichi_next = [false; 4];
    let mut wins: Vec<&Agari> = Vec::new();
    let mut over = false;

    for (i, e) in events.iter().enumerate() {
        let fail = |m: String| (i, m);
        let act = |g: &mut State, seat: u8, a: Action, out: &mut Vec<Event>| {
            g.step(seat, a, out).map_err(|err| {
                fail(format!(
                    "seat {seat} {a:?} rejected ({err:?}); legal: {:?}",
                    &g.legal_actions(seat)[..]
                ))
            })
        };
        match e {
            LogEvent::Init(_) => return Err(fail("INIT inside a hand".into())),
            LogEvent::Draw { seat, tile } => {
                pass_all(&mut g, &mut out).map_err(fail)?;
                let want = tile_of(*tile, red);
                if g.phase != (Phase::Turn { seat: *seat }) {
                    return Err(fail(format!(
                        "log: seat {seat} draws {want}; engine phase {:?}",
                        g.phase
                    )));
                }
                let drawn = g.players[*seat as usize].drawn;
                if drawn != want.code() {
                    return Err(fail(format!(
                        "seat {seat} drew code {drawn}, log says {want}"
                    )));
                }
            }
            LogEvent::Discard { seat, tile } => {
                let t = tile_of(*tile, red);
                let s = *seat as usize;
                let a = if riichi_next[s] {
                    Action::Riichi(t)
                } else {
                    Action::Discard(t)
                };
                riichi_next[s] = false;
                act(&mut g, *seat, a, &mut out)?;
            }
            LogEvent::Riichi { seat, step } => {
                if *step == 1 {
                    riichi_next[*seat as usize] = true;
                }
            }
            LogEvent::Dora { .. } => {}
            LogEvent::Call { seat, call } => {
                let hand: Vec<u8> = call
                    .from_hand()
                    .iter()
                    .map(|&t| tile_of(t, red).code())
                    .collect();
                let same = |ts: &[Tile; 2]| {
                    let mut a = [ts[0].code(), ts[1].code()];
                    a.sort();
                    let mut b = [hand[0], hand[1]];
                    b.sort();
                    a == b
                };
                let kind = call.tiles[0] / 4;
                let legal = g.legal_actions(*seat);
                let found = legal.iter().copied().find(|a| match (call.kind, a) {
                    (CallKind::Chi, Action::Chi(ts)) => same(ts),
                    (CallKind::Pon, Action::Pon(ts)) => same(ts),
                    (CallKind::Daiminkan, Action::Daiminkan) => true,
                    (CallKind::Ankan, Action::Ankan(k)) => *k == kind,
                    (CallKind::Kakan, Action::Kakan(t)) => t.code() == hand[0],
                    _ => false,
                });
                let Some(a) = found else {
                    return Err(fail(format!(
                        "seat {seat} {:?} with {hand:?} is not legal; legal: {:?}",
                        call.kind,
                        &legal[..]
                    )));
                };
                act(&mut g, *seat, a, &mut out)?;
                if matches!(
                    call.kind,
                    CallKind::Chi | CallKind::Pon | CallKind::Daiminkan
                ) {
                    pass_all(&mut g, &mut out).map_err(fail)?;
                }
            }
            LogEvent::Agari(a) => {
                let action = if a.who == a.from {
                    Action::Tsumo
                } else {
                    Action::Ron
                };
                act(&mut g, a.who, action, &mut out)?;
                wins.push(a);
                if !matches!(events.get(i + 1), Some(LogEvent::Agari(_))) {
                    pass_all(&mut g, &mut out).map_err(fail)?;
                }
            }
            LogEvent::Ryuukyoku(r) => {
                match r.kind {
                    DrawKind::Kyuushu => {
                        let Phase::Turn { seat } = g.phase else {
                            return Err(fail(format!("kyuushu in phase {:?}", g.phase)));
                        };
                        act(&mut g, seat, Action::Kyuushu, &mut out)?;
                    }
                    DrawKind::Sanchahou => {
                        let ron: Vec<u8> = (0..4)
                            .filter(|&s| g.legal_actions(s).contains(&Action::Ron))
                            .collect();
                        if ron.len() != 3 {
                            return Err(fail(format!(
                                "triple ron, but the engine offers ron to {ron:?}"
                            )));
                        }
                        for s in ron {
                            act(&mut g, s, Action::Ron, &mut out)?;
                        }
                    }
                    _ => pass_all(&mut g, &mut out).map_err(fail)?,
                }
                check_hand_end(&g, &out, &[], r.deltas).map_err(fail)?;
                over = true;
            }
        }
        if !wins.is_empty() && !matches!(events.get(i + 1), Some(LogEvent::Agari(_))) {
            let deltas = wins.iter().fold([0; 4], |mut d, a| {
                for (x, y) in d.iter_mut().zip(a.deltas) {
                    *x += y;
                }
                d
            });
            check_hand_end(&g, &out, &wins, deltas).map_err(fail)?;
            over = true;
        }
        if over {
            if let Some(extra) = events.get(i + 1) {
                return Err((i + 1, format!("event after the hand ended: {extra:?}")));
            }
            return Ok(Some(g));
        }
    }
    Ok(None)
}

/// Every seat still able to answer a call or chankan window passes.
fn pass_all(g: &mut State, out: &mut Vec<Event>) -> Result<(), String> {
    while let Phase::CallWindow { pending, .. } | Phase::ChankanWindow { pending, .. } = g.phase {
        if pending == 0 {
            return Err(format!("window with nobody pending: {:?}", g.phase));
        }
        let seat = pending.trailing_zeros() as u8;
        g.step(seat, Action::Pass, out)
            .map_err(|e| format!("seat {seat} can't pass: {e:?}"))?;
    }
    Ok(())
}

/// The hand must be over, with the logged wins and score changes.
fn check_hand_end(
    g: &State,
    out: &[Event],
    wins: &[&Agari],
    log_deltas: [i32; 4],
) -> Result<(), String> {
    if !matches!(g.phase, Phase::RoundEnd | Phase::GameEnd) {
        return Err(format!("log ends the hand, engine phase {:?}", g.phase));
    }
    let engine_wins: Vec<(u8, u8, u8, u32, u8)> = out
        .iter()
        .filter_map(|e| match *e {
            Event::Win {
                seat,
                han,
                fu,
                points,
                yakuman,
                ..
            } => Some((seat, han, fu, points, yakuman)),
            _ => None,
        })
        .collect();
    let mut log_wins: Vec<_> = wins
        .iter()
        .map(|a| (a.who, a.han(), a.fu, a.points))
        .collect();
    let mut got: Vec<_> = engine_wins
        .iter()
        .map(|&(seat, han, fu, points, yakuman)| {
            let han = if yakuman > 0 {
                13 * yakuman as u32
            } else {
                han as u32
            };
            (seat, han, fu as u32, points)
        })
        .collect();
    // Tenhou writes a fu value for yakuman too; only compare fu below them.
    for w in log_wins.iter_mut().chain(got.iter_mut()) {
        if w.1 >= 13 {
            w.2 = 0;
        }
    }
    log_wins.sort();
    got.sort();
    if log_wins != got {
        return Err(format!(
            "wins (seat, han, fu, points): log {log_wins:?}, engine {got:?}"
        ));
    }
    let deltas = out.iter().rev().find_map(|e| match e {
        Event::RoundEnded { deltas } => Some(*deltas),
        _ => None,
    });
    if deltas != Some(log_deltas) {
        return Err(format!("deltas: log {log_deltas:?}, engine {deltas:?}"));
    }
    Ok(())
}

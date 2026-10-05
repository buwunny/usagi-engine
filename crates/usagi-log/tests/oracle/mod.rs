//! Drives riichienv-core alongside the usagi engine during log replay and
//! compares the full set of legal actions at every decision point.
//!
//! Shared by `tests/riichienv_compare.rs` and
//! `examples/compare-riichienv.rs`.
//!
//! The replayer reports each deal and step ([`ReplayEvent`]). For each
//! hand, riichienv gets the same wall (log tile ids, rearranged into its
//! layout) and the same moves. Decision points are compared like this:
//!
//! - **Turn** (after a draw or a call): the acting seat's actions.
//!   riichienv offers riichi as one action and asks for the discard
//!   afterwards, so its riichi discards are read by declaring riichi on a
//!   copy of its state.
//! - **Call window** (after a discard or a kakan): every other seat's
//!   calls, compared when the window opens. Pass is left out on both
//!   sides. If one engine opens a window and the other doesn't, the
//!   window is compared against nothing.
//!
//! Actions are compared as tile codes (red fives distinct), since one
//! engine tracks copies of a kind and the other doesn't. When riichienv
//! doesn't allow the logged move, the rest of that hand can't be played
//! on it and is skipped.

// The test and the example each use part of this module.
#![allow(dead_code)]

use std::collections::{BTreeSet, HashMap};
use std::fmt;

use riichienv_core::action::{Action as RvAction, ActionType, Phase as RvPhase};
use riichienv_core::rule::GameRule;
use riichienv_core::state::GameState as RvState;
use riichienv_core::state::legal_actions::GameStateLegalActions;
use usagi_core::Tile;
use usagi_engine::wall::{DORA_START, RINSHAN_START, URA_START, WALL_SIZE};
use usagi_engine::{Action, GameState, Phase};
use usagi_log::ReplayEvent;
use usagi_log::mjlog::{Init, TileId, tile_of};

type State = GameState;

/// One action, engine-neutral.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Norm {
    Discard(Tile),
    Riichi(Tile),
    Tsumo,
    Ron,
    Chi(Tile, Tile),
    Pon(Tile, Tile),
    Daiminkan,
    /// Kind of the kan.
    Ankan(u8),
    /// Kind of the kan.
    Kakan(u8),
    Kyuushu,
}

impl fmt::Display for Norm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let k = |k: &u8| Tile::from_kind(*k);
        match self {
            Norm::Discard(t) => write!(f, "discard {t}"),
            Norm::Riichi(t) => write!(f, "riichi {t}"),
            Norm::Tsumo => write!(f, "tsumo"),
            Norm::Ron => write!(f, "ron"),
            Norm::Chi(a, b) => write!(f, "chi {a}{b}"),
            Norm::Pon(a, b) => write!(f, "pon {a}{b}"),
            Norm::Daiminkan => write!(f, "daiminkan"),
            Norm::Ankan(x) => write!(f, "ankan {}", k(x)),
            Norm::Kakan(x) => write!(f, "kakan {}", k(x)),
            Norm::Kyuushu => write!(f, "kyuushu"),
        }
    }
}

fn pair(a: Tile, b: Tile) -> (Tile, Tile) {
    if a <= b { (a, b) } else { (b, a) }
}

fn norm_usagi(a: Action) -> Option<Norm> {
    Some(match a {
        Action::Discard(t) => Norm::Discard(t),
        Action::Riichi(t) => Norm::Riichi(t),
        Action::Tsumo => Norm::Tsumo,
        Action::Ron => Norm::Ron,
        Action::Chi([a, b]) => {
            let (a, b) = pair(a, b);
            Norm::Chi(a, b)
        }
        Action::Pon([a, b]) => {
            let (a, b) = pair(a, b);
            Norm::Pon(a, b)
        }
        Action::Daiminkan => Norm::Daiminkan,
        Action::Ankan(k) => Norm::Ankan(k),
        Action::Kakan(t) => Norm::Kakan(t.kind()),
        Action::Kyuushu => Norm::Kyuushu,
        Action::Pass => return None,
    })
}

fn tile(id: u8) -> Tile {
    tile_of(id, true)
}

/// `None` for pass, and for riichi (expanded separately).
fn norm_rv(a: &RvAction) -> Option<Norm> {
    let t = || tile(a.tile.expect("action has a tile"));
    let c = |i: usize| tile(a.consume_tiles[i]);
    Some(match a.action_type {
        ActionType::Discard => Norm::Discard(t()),
        ActionType::Tsumo => Norm::Tsumo,
        ActionType::Ron => Norm::Ron,
        ActionType::Chi => {
            let (a, b) = pair(c(0), c(1));
            Norm::Chi(a, b)
        }
        ActionType::Pon => {
            let (a, b) = pair(c(0), c(1));
            Norm::Pon(a, b)
        }
        ActionType::Daiminkan => Norm::Daiminkan,
        ActionType::Ankan => Norm::Ankan(c(0).kind()),
        ActionType::Kakan => Norm::Kakan(t().kind()),
        ActionType::KyushuKyuhai => Norm::Kyuushu,
        ActionType::Riichi | ActionType::Pass | ActionType::Kita => return None,
    })
}

/// One decision point where the engines disagree.
#[derive(Clone, Debug)]
pub struct Diff {
    pub hand: usize,
    pub round: u8,
    pub honba: u8,
    pub seat: u8,
    /// "turn" or "call".
    pub point: &'static str,
    pub usagi_only: Vec<Norm>,
    pub rv_only: Vec<Norm>,
    /// What the log did there, if this seat acted.
    pub logged: Option<Action>,
}

impl Diff {
    /// A short label to group diffs by: the point and which actions
    /// (without tiles) each side has alone.
    pub fn category(&self) -> String {
        let kinds = |v: &[Norm]| {
            let mut k: Vec<&str> = v
                .iter()
                .map(|n| match n {
                    Norm::Discard(_) => "discard",
                    Norm::Riichi(_) => "riichi",
                    Norm::Tsumo => "tsumo",
                    Norm::Ron => "ron",
                    Norm::Chi(..) => "chi",
                    Norm::Pon(..) => "pon",
                    Norm::Daiminkan => "daiminkan",
                    Norm::Ankan(_) => "ankan",
                    Norm::Kakan(_) => "kakan",
                    Norm::Kyuushu => "kyuushu",
                })
                .collect();
            k.dedup();
            k.join("+")
        };
        format!(
            "{}: usagi only [{}], riichienv only [{}]",
            self.point,
            kinds(&self.usagi_only),
            kinds(&self.rv_only)
        )
    }
}

impl fmt::Display for Diff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const WINDS: [&str; 3] = ["East", "South", "West"];
        let list = |v: &[Norm]| {
            v.iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        write!(
            f,
            "hand {} ({} {}-{}), seat {} {}: usagi only [{}], riichienv only [{}]",
            self.hand,
            WINDS.get(self.round as usize / 4).unwrap_or(&"?"),
            self.round % 4 + 1,
            self.honba,
            self.seat,
            self.point,
            list(&self.usagi_only),
            list(&self.rv_only),
        )?;
        if let Some(a) = self.logged {
            write!(f, "; logged {a:?}")?;
        }
        Ok(())
    }
}

/// Counts and differences for a run.
#[derive(Clone, Debug, Default)]
pub struct Comparison {
    /// Decision points compared (turns, plus seats in call windows).
    pub points: u64,
    pub hands: u64,
    /// Hands where riichienv couldn't follow the log to the end.
    pub hands_cut_short: u64,
    pub diffs: Vec<Diff>,
    /// Why each cut-short hand stopped.
    pub cut_reasons: Vec<String>,
}

/// The riichienv side of one hand.
struct Hand {
    rv: RvState,
    hand: usize,
    round: u8,
    honba: u8,
    /// riichienv is in a call window nobody has compared yet.
    window_unchecked: bool,
    /// Responses collected in the current call window.
    responses: HashMap<u8, RvAction>,
    in_window: bool,
}

/// Plays riichienv alongside a replay. Feed it every [`ReplayEvent`].
pub struct Oracle {
    cur: Option<Hand>,
    pub result: Comparison,
}

impl Default for Oracle {
    fn default() -> Self {
        Oracle::new()
    }
}

impl Oracle {
    pub fn new() -> Oracle {
        Oracle {
            cur: None,
            result: Comparison::default(),
        }
    }

    pub fn on_event(&mut self, ev: ReplayEvent) {
        match ev {
            ReplayEvent::Deal {
                hand, init, wall, ..
            } => {
                self.result.hands += 1;
                self.cur = Some(Hand::new(hand, init, wall));
            }
            ReplayEvent::Step {
                state,
                seat,
                action,
            } => {
                let Some(h) = self.cur.as_mut() else { return };
                if let Err(why) = h.step(state, seat, action, &mut self.result) {
                    self.result.hands_cut_short += 1;
                    self.result.cut_reasons.push(format!(
                        "hand {} round {} honba {}: {why}",
                        h.hand, h.round, h.honba
                    ));
                    self.cur = None;
                }
            }
            ReplayEvent::HandDone { ended, .. } => {
                if let Some(h) = self.cur.as_mut()
                    && ended
                {
                    h.finish(&mut self.result);
                }
                self.cur = None;
            }
        }
    }
}

/// riichienv's wall vector for a wall in usagi's layout.
///
/// riichienv keeps the wall as a stack: `load_wall` reverses its input
/// into `S`, deals and live draws pop from the end of `S` (four tiles per
/// seat three times, then one each), rinshan draws take `S[0..4]` in
/// order, dora indicators are `S[4], S[6], ..` and ura `S[5], S[7], ..`.
fn rv_wall(ours: &[TileId; WALL_SIZE]) -> Vec<u8> {
    let mut s = [0u8; WALL_SIZE];
    for r in 0..3 {
        for p in 0..4 {
            for j in 0..4 {
                let n = r * 16 + p * 4 + j;
                s[135 - n] = ours[p * 13 + r * 4 + j];
            }
        }
    }
    for p in 0..4 {
        s[135 - (48 + p)] = ours[p * 13 + 12];
    }
    for k in 0..70 {
        s[135 - 52 - k] = ours[52 + k];
    }
    for i in 0..4 {
        s[i] = ours[RINSHAN_START as usize + i];
    }
    for i in 0..5 {
        s[4 + 2 * i] = ours[DORA_START as usize + i];
        s[5 + 2 * i] = ours[URA_START as usize + i];
    }
    let mut v = s.to_vec();
    v.reverse();
    v
}

/// Tenhou's rules as riichienv spells them. riichienv's own Tenhou preset
/// doesn't let kokushi rob a closed kan, but Tenhou does.
fn tenhou_rule() -> GameRule {
    GameRule {
        allows_ron_on_ankan_for_kokushi_musou: true,
        ..GameRule::default_tenhou()
    }
}

impl Hand {
    fn new(hand: usize, init: &Init, wall: &[TileId; WALL_SIZE]) -> Hand {
        let wind = init.round / 4;
        let mut rv = RvState::new(2, true, Some(0), wind, tenhou_rule());
        rv._initialize_round(
            init.round % 4,
            wind,
            init.honba,
            init.sticks as u32,
            Some(rv_wall(wall)),
            Some(init.scores.to_vec()),
        );
        Hand {
            rv,
            hand,
            round: init.round,
            honba: init.honba,
            window_unchecked: false,
            responses: HashMap::new(),
            in_window: false,
        }
    }

    fn diff(
        &self,
        seat: u8,
        point: &'static str,
        ours: BTreeSet<Norm>,
        theirs: BTreeSet<Norm>,
        logged: Option<Action>,
        out: &mut Comparison,
    ) {
        out.points += 1;
        if ours == theirs {
            return;
        }
        out.diffs.push(Diff {
            hand: self.hand,
            round: self.round,
            honba: self.honba,
            seat,
            point,
            usagi_only: ours.difference(&theirs).copied().collect(),
            rv_only: theirs.difference(&ours).copied().collect(),
            logged,
        });
    }

    /// riichienv's legal actions for `seat`, riichi discards included.
    fn rv_legal(&self, seat: u8) -> BTreeSet<Norm> {
        let legal = self.rv._get_legal_actions_internal(seat);
        let mut set: BTreeSet<Norm> = legal.iter().filter_map(norm_rv).collect();
        if legal.iter().any(|a| a.action_type == ActionType::Riichi) {
            let mut copy = self.rv.clone();
            copy.step(&HashMap::from([(
                seat,
                rv_action(ActionType::Riichi, None, seat),
            )]));
            for a in copy._get_legal_actions_internal(seat) {
                if a.action_type == ActionType::Discard {
                    set.insert(Norm::Riichi(tile(a.tile.unwrap())));
                }
            }
        }
        set
    }

    /// The seats riichienv's open call window is asking.
    fn rv_window_seats(&self) -> Vec<u8> {
        if self.rv.phase == RvPhase::WaitResponse && !self.rv.is_done {
            let mut s = self.rv.active_players.clone();
            s.sort();
            s
        } else {
            Vec::new()
        }
    }

    /// Compares a call window that only riichienv opened.
    fn check_rv_only_window(&mut self, out: &mut Comparison) {
        if !self.window_unchecked {
            return;
        }
        self.window_unchecked = false;
        for seat in self.rv_window_seats() {
            self.diff(
                seat,
                "call",
                BTreeSet::new(),
                self.rv_legal(seat),
                None,
                out,
            );
        }
    }

    /// Everyone left in riichienv's call window passes.
    fn close_rv_window(&mut self) {
        let responses = std::mem::take(&mut self.responses);
        if self.rv.phase == RvPhase::WaitResponse && !self.rv.is_done {
            self.rv.step(&responses);
            self.after_rv_step();
        }
        self.in_window = false;
    }

    fn after_rv_step(&mut self) {
        if let Some(e) = &self.rv.last_error {
            panic!("riichienv error after a legal move: {e}");
        }
        self.window_unchecked = self.rv.phase == RvPhase::WaitResponse && !self.rv.is_done;
    }

    fn step(
        &mut self,
        g: &State,
        seat: u8,
        action: Action,
        out: &mut Comparison,
    ) -> Result<(), String> {
        let window = matches!(
            g.phase,
            Phase::CallWindow { .. } | Phase::ChankanWindow { .. }
        );
        if !window {
            if self.in_window {
                self.close_rv_window();
            }
            self.check_rv_only_window(out);
            if self.window_unchecked {
                // riichienv waits on calls the usagi engine never offered.
                self.close_rv_window();
            }
            let ours = usagi_set(g, seat);
            let rv_turn = self.rv.phase == RvPhase::WaitAct
                && self.rv.current_player == seat
                && !self.rv.is_done;
            let theirs = if rv_turn {
                self.rv_legal(seat)
            } else {
                BTreeSet::new()
            };
            self.diff(seat, "turn", ours, theirs, Some(action), out);
            if !rv_turn {
                return Err(format!(
                    "riichienv isn't waiting on seat {seat} (phase {:?}, player {})",
                    self.rv.phase, self.rv.current_player
                ));
            }
            return self.rv_turn(seat, action);
        }

        // A call window. Compare every seat when it opens.
        if !self.in_window {
            self.in_window = true;
            let rv_seats = self.rv_window_seats();
            self.window_unchecked = false;
            let (Phase::CallWindow { pending, .. } | Phase::ChankanWindow { pending, .. }) =
                g.phase
            else {
                unreachable!()
            };
            for s in 0..4u8 {
                let ours = if pending & (1 << s) != 0 {
                    usagi_set(g, s)
                } else {
                    BTreeSet::new()
                };
                let theirs = if rv_seats.contains(&s) {
                    self.rv_legal(s)
                } else {
                    BTreeSet::new()
                };
                if ours.is_empty() && theirs.is_empty() {
                    continue;
                }
                let logged = (s == seat).then_some(action);
                self.diff(s, "call", ours, theirs, logged, out);
            }
        }
        if action == Action::Pass {
            return Ok(());
        }
        let Some(want) = norm_usagi(action) else {
            unreachable!()
        };
        let legal = self.rv._get_legal_actions_internal(seat);
        let Some(a) = legal.iter().find(|a| norm_rv(a) == Some(want)) else {
            return Err(format!("riichienv doesn't allow seat {seat} {want}"));
        };
        self.responses.insert(seat, a.clone());
        Ok(())
    }

    /// Plays a turn action on riichienv.
    fn rv_turn(&mut self, seat: u8, action: Action) -> Result<(), String> {
        let legal = self.rv._get_legal_actions_internal(seat);
        let drawn = self.rv.drawn_tile;
        let pick = |legal: &[RvAction], want: Norm| -> Result<RvAction, String> {
            let matching: Vec<&RvAction> =
                legal.iter().filter(|a| norm_rv(a) == Some(want)).collect();
            // Prefer the drawn tile (tsumogiri) among equal copies.
            matching
                .iter()
                .find(|a| a.tile.is_some() && a.tile == drawn)
                .or(matching.first())
                .map(|a| (*a).clone())
                .ok_or_else(|| format!("riichienv doesn't allow seat {seat} {want}"))
        };
        let want = norm_usagi(action).expect("not a pass");
        if let Norm::Riichi(t) = want {
            if !legal.iter().any(|a| a.action_type == ActionType::Riichi) {
                return Err(format!("riichienv doesn't allow seat {seat} riichi"));
            }
            self.rv.step(&HashMap::from([(
                seat,
                rv_action(ActionType::Riichi, None, seat),
            )]));
            self.after_rv_step();
            let legal = self.rv._get_legal_actions_internal(seat);
            let a = pick(&legal, Norm::Discard(t))?;
            self.rv.step(&HashMap::from([(seat, a)]));
        } else {
            let a = pick(&legal, want)?;
            self.rv.step(&HashMap::from([(seat, a)]));
        }
        self.after_rv_step();
        Ok(())
    }

    /// The hand is over on the usagi side: compare a window only riichienv
    /// opened at the very end (the last discard of the wall, say).
    fn finish(&mut self, out: &mut Comparison) {
        if !self.in_window {
            self.check_rv_only_window(out);
        }
    }
}

fn rv_action(kind: ActionType, tile: Option<u8>, seat: u8) -> RvAction {
    RvAction::new(kind, tile, vec![], Some(seat))
}

fn usagi_set(g: &State, seat: u8) -> BTreeSet<Norm> {
    g.legal_actions(seat)
        .iter()
        .filter_map(|&a| norm_usagi(a))
        .collect()
}

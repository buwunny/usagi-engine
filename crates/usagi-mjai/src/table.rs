//! A table that runs a game for four Mjai bots.
//!
//! The protocol is the one mjai.app and Mortal use: whenever a bot has a
//! decision to make, it is sent one line holding a JSON array of every
//! event since its last message (other seats' draws and starting hands
//! hidden), and it answers with one JSON object: its action, or
//! `{"type":"none"}` to pass. Riichi takes two answers: `reach`, then
//! (after the table echoes the `reach` event back) the `dahai`.
//!
//! The engine checks every answer; an illegal one stops the game with a
//! [`TableError`] naming the seat.

use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use usagi_engine::game::Event as EngineEvent;
use usagi_engine::{Action, GameState, Phase, TenhouRules};

use crate::convert::{convert, mask, start_kyoku};
use crate::event::Event;

/// Anything that can sit at the table.
pub trait Bot {
    /// `events` happened since the last call; return the bot's decision
    /// for the last of them (or [`Event::None`]).
    fn react(&mut self, events: &[Event]) -> Result<Event, String>;
}

#[derive(Debug)]
pub struct TableError {
    /// The seat at fault, if the problem was a bot's answer.
    pub seat: Option<u8>,
    pub message: String,
    /// Every event up to the failure, with full information.
    pub log: Vec<Event>,
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.seat {
            Some(s) => write!(f, "seat {s}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for TableError {}

/// A finished game.
#[derive(Clone, Debug)]
pub struct GameRecord {
    pub scores: [i32; 4],
    /// Every event with full information (an Mjai log).
    pub log: Vec<Event>,
}

type State = GameState<TenhouRules>;

struct Table<'a> {
    g: State,
    bots: [&'a mut dyn Bot; 4],
    log: Vec<Event>,
    /// Events each seat hasn't been sent yet, already masked.
    pending: [Vec<Event>; 4],
    /// The seat whose `reach` was already echoed to it.
    reach_echoed: Option<u8>,
}

/// Plays one game with walls from `seed`. Seat 0 deals first.
pub fn play_game(seed: u64, bots: [&mut dyn Bot; 4]) -> Result<GameRecord, TableError> {
    let mut t = Table {
        g: State::new(seed),
        bots,
        log: Vec::new(),
        pending: Default::default(),
        reach_echoed: None,
    };
    for s in 0..4u8 {
        t.pending[s as usize].push(Event::StartGame {
            id: Some(s),
            names: Vec::new(),
        });
    }
    t.log.push(Event::StartGame {
        id: None,
        names: Vec::new(),
    });
    let first = vec![start_kyoku(&t.g), {
        let dealer = t.g.dealer();
        let tile = usagi_core::Tile::from_code(t.g.players[dealer as usize].drawn).unwrap();
        Event::Tsumo {
            actor: dealer,
            pai: tile.into(),
        }
    }];
    for e in first {
        t.broadcast(e);
    }
    loop {
        match t.g.phase {
            Phase::GameEnd => break,
            Phase::RoundEnd => {
                let mut evs = Vec::new();
                t.g.start_next_round(&mut evs)
                    .map_err(|e| t.error(None, format!("start_next_round: {e:?}")))?;
                t.dispatch(&evs);
                continue;
            }
            _ => {}
        }
        let seat = t.g.waiting_on().trailing_zeros() as u8;
        let action = t.ask(seat)?;
        let mut evs = Vec::new();
        t.g.step(seat, action, &mut evs).map_err(|e| {
            t.error(
                Some(seat),
                format!("{action:?} rejected by the engine ({e:?})"),
            )
        })?;
        t.dispatch(&evs);
        t.reach_echoed = None;
    }
    // Let every bot see the end of the game; their answers don't matter.
    for s in 0..4u8 {
        let events = std::mem::take(&mut t.pending[s as usize]);
        let _ = t.bots[s as usize].react(&events);
    }
    Ok(GameRecord {
        scores: t.g.scores,
        log: t.log,
    })
}

impl Table<'_> {
    fn error(&self, seat: Option<u8>, message: String) -> TableError {
        TableError {
            seat,
            message,
            log: self.log.clone(),
        }
    }

    fn broadcast(&mut self, e: Event) {
        for s in 0..4u8 {
            if matches!(e, Event::Reach { actor } if Some(actor) == self.reach_echoed && actor == s)
            {
                continue;
            }
            self.pending[s as usize].push(mask(s, &e));
        }
        self.log.push(e);
    }

    fn dispatch(&mut self, evs: &[EngineEvent]) {
        let mut out = Vec::new();
        for e in evs {
            convert(&self.g, e, &mut out);
        }
        for e in out {
            self.broadcast(e);
        }
    }

    /// Sends `seat` its pending events and turns its answer into an action.
    fn ask(&mut self, seat: u8) -> Result<Action, TableError> {
        let events = std::mem::take(&mut self.pending[seat as usize]);
        let mut answer = self.bots[seat as usize]
            .react(&events)
            .map_err(|e| self.error(Some(seat), format!("bot failed: {e}")))?;
        let mut riichi = false;
        if matches!(answer, Event::Reach { .. }) && matches!(self.g.phase, Phase::Turn { .. }) {
            riichi = true;
            self.reach_echoed = Some(seat);
            answer = self.bots[seat as usize]
                .react(&[Event::Reach { actor: seat }])
                .map_err(|e| self.error(Some(seat), format!("bot failed: {e}")))?;
        }
        to_action(&self.g, seat, &answer, riichi)
            .ok_or_else(|| self.error(Some(seat), format!("illegal answer {answer:?}")))
    }
}

/// The engine action for a bot's answer, if it names a legal one.
pub fn to_action(g: &State, seat: u8, answer: &Event, riichi: bool) -> Option<Action> {
    let legal = g.legal_actions(seat);
    let codes = |ps: &[crate::event::Pai]| -> Option<Vec<u8>> {
        let mut v: Vec<u8> = ps
            .iter()
            .map(|p| p.tile().map(|t| t.code()))
            .collect::<Option<_>>()?;
        v.sort();
        Some(v)
    };
    let pair = |ts: &[usagi_core::Tile; 2]| {
        let mut v = vec![ts[0].code(), ts[1].code()];
        v.sort();
        v
    };
    let action = match answer {
        Event::Dahai { pai, .. } => {
            let t = pai.tile()?;
            if riichi {
                Action::Riichi(t)
            } else {
                Action::Discard(t)
            }
        }
        Event::Chi { consumed, .. } => {
            let want = codes(consumed)?;
            *legal
                .iter()
                .find(|a| matches!(a, Action::Chi(ts) if pair(ts) == want))?
        }
        Event::Pon { consumed, .. } => {
            let want = codes(consumed)?;
            *legal
                .iter()
                .find(|a| matches!(a, Action::Pon(ts) if pair(ts) == want))?
        }
        Event::Daiminkan { .. } => Action::Daiminkan,
        Event::Ankan { consumed, .. } => Action::Ankan(consumed.first()?.tile()?.kind()),
        Event::Kakan { pai, .. } => Action::Kakan(pai.tile()?),
        Event::Hora { .. } => {
            if matches!(g.phase, Phase::Turn { .. }) {
                Action::Tsumo
            } else {
                Action::Ron
            }
        }
        Event::Ryukyoku { .. } => Action::Kyuushu,
        Event::None => Action::Pass,
        _ => return None,
    };
    legal.contains(&action).then_some(action)
}

/// A bot in another process, speaking the protocol over stdin/stdout.
pub struct ProcessBot {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl ProcessBot {
    /// Runs `command` with `sh -c`.
    pub fn spawn(command: &str) -> std::io::Result<ProcessBot> {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
        Ok(ProcessBot {
            child,
            stdin,
            stdout,
        })
    }
}

impl Bot for ProcessBot {
    fn react(&mut self, events: &[Event]) -> Result<Event, String> {
        let line = serde_json::to_string(events).map_err(|e| e.to_string())?;
        writeln!(self.stdin, "{line}").map_err(|e| format!("write: {e}"))?;
        self.stdin.flush().map_err(|e| format!("flush: {e}"))?;
        let mut answer = String::new();
        if self
            .stdout
            .read_line(&mut answer)
            .map_err(|e| format!("read: {e}"))?
            == 0
        {
            return Err("bot closed its output".into());
        }
        serde_json::from_str(answer.trim()).map_err(|e| format!("bad answer {answer:?}: {e}"))
    }
}

impl Drop for ProcessBot {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

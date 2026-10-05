//! # usagi-engine
//!
//! The game itself: wall, players, turn order, legal actions and what
//! happens after each one, for four-player Tenhou rules.
//!
//! Everything goes through two methods on [`GameState`]:
//! - [`GameState::legal_actions`]: what may `seat` do right now?
//! - [`GameState::step`]: `seat` does `action`; update the state and
//!   report [`game::Event`]s.
//!
//! When a hand ends the phase becomes [`Phase::RoundEnd`]; deal the next
//! one with [`GameState::start_next_round`] (seeded wall) or
//! [`GameState::start_next_round_with_wall`] (a given wall).

pub mod action;
pub mod game;
pub mod phase;
pub mod rng;
pub mod rules;
pub mod state;
pub mod wall;

pub use action::{Action, ActionList};
pub use game::{AbortKind, Event, StepError};
pub use phase::Phase;
pub use rules::{Rules, TenhouRules};
pub use state::{GameState, PlayerState, Round};

//! # usagi-mjai
//!
//! The [Mjai](https://github.com/gimite/mjai) protocol, so usagi engine can
//! host and play against other Mahjong bots such as Mortal.
//!
//! - [`event`]: Mjai messages and tile strings.
//! - [`convert`]: engine events to Mjai events, and hiding what a seat
//!   can't see.
//! - [`table`]: runs a game for four bots (in-process or child processes
//!   speaking the mjai.app line protocol) and checks every answer.
//! - [`baseline`]: a rule-based bot (lowest shanten, riichi when tenpai,
//!   fold against riichi).
//! - [`view`]: what one seat knows, rebuilt from its events.
//! - [`rulebot`]: rule-based bots at three strengths for usagi.club, and
//!   [`suggest`] for move hints.
//! - [`explain`]: why a bot made its last move, as a structured record
//!   any bot (rule-based now, bunny bot later) can fill.

pub mod baseline;
pub mod convert;
pub mod event;
pub mod explain;
pub mod rulebot;
pub mod table;
pub mod view;

pub use baseline::Baseline;
pub use event::{Event, Pai};
pub use explain::{Explain, Explanation, SiteBot};
pub use rulebot::{Level, RuleBot, suggest};
pub use table::{Bot, GameRecord, ProcessBot, TableError, play_game};

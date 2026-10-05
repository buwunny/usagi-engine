//! # mochitsuki-mjai
//!
//! The [Mjai](https://github.com/gimite/mjai) protocol, so mochitsuki can
//! host and play against other Mahjong bots such as Mortal.
//!
//! - [`event`]: Mjai messages and tile strings.
//! - [`convert`]: engine events to Mjai events, and hiding what a seat
//!   can't see.
//! - [`table`]: runs a game for four bots (in-process or child processes
//!   speaking the mjai.app line protocol) and checks every answer.
//! - [`baseline`]: a rule-based bot (lowest shanten, riichi when tenpai,
//!   fold against riichi).

pub mod baseline;
pub mod convert;
pub mod event;
pub mod table;

pub use baseline::Baseline;
pub use event::{Event, Pai};
pub use table::{Bot, GameRecord, ProcessBot, TableError, play_game};

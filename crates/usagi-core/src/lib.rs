//! # usagi-core
//!
//! Pure rules logic for Tenhou-rules Riichi Mahjong: tiles, hand counts,
//! shanten, waits, hand decomposition, yaku, fu and scoring.
//!
//! Nothing here knows about turns, players or a wall: every function takes
//! a hand (and maybe some context) and returns an answer.
//!
//! | Module      | Contents                                         |
//! |-------------|--------------------------------------------------|
//! | [`tile`]    | the 37 tile codes (34 kinds + 3 red fives)       |
//! | [`counts`]  | 34-slot hand counts                              |
//! | [`parse`]   | `123m456p789s11z` hand strings                   |
//! | [`kindset`] | sets of kinds as a 64-bit mask                   |
//! | [`shanten`] | shanten (table-driven, with a slow reference)    |
//! | [`waits`]   | waits and tile acceptance                        |
//! | [`hand`]    | melds, decompositions, readings, wait shapes     |
//! | [`yaku`]    | yaku detection and the win context               |
//! | [`fu`]      | fu                                               |
//! | [`score`]   | base points, payments, score deltas, `score()`   |

pub mod counts;
pub mod fu;
pub mod hand;
pub mod kindset;
pub mod parse;
pub mod score;
pub mod shanten;
pub mod tile;
pub mod waits;
pub mod yaku;

pub use counts::Counts;
pub use kindset::KindSet;
pub use parse::{counts_of, format_tiles, parse_tiles, tiles_of};
pub use tile::{Suit, Tile};

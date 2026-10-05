//! # usagi-obs
//!
//! What a network sees and chooses from:
//!
//! - [`encode`]: one seat's [`Observation`], built only from what that seat
//!   may know (its own hand, everything public, never the wall or another
//!   seat's concealed tiles).
//! - [`action_space`]: a fixed numbering of [`usagi_engine::Action`]s
//!   and the legal-action mask.

pub mod action_space;
pub mod encode;

pub use action_space::{NUM_ACTIONS, action_at, index_of, legal_mask};
pub use encode::{Decision, Observation, VERSION, encode, encode_into};

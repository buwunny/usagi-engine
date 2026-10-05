//! # usagi-log
//!
//! Reads Tenhou game logs (mjlog XML) and replays them through the
//! engine.
//!
//! - [`mjlog`]: the log format as typed events ([`mjlog::parse`]).
//! - [`replay`]: feeds a parsed game through `usagi-engine` and
//!   reports the first mismatch in each hand ([`replay::replay`]).
//! - [`read_log`]: loads a log file, gzipped or not.

pub mod mjlog;
pub mod replay;
pub mod source;
pub mod xml;

use std::io::Read;
use std::path::Path;

pub use mjlog::{Game, LogError, LogEvent, parse};
pub use replay::{Mismatch, Options, ReplayEvent, Report, replay, replay_with};

/// Reads an mjlog file. Gzip-compressed files (as Tenhou serves them) are
/// detected by their magic bytes and decompressed.
pub fn read_log(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut s = String::new();
        flate2::read::GzDecoder::new(&bytes[..]).read_to_string(&mut s)?;
        Ok(s)
    } else {
        String::from_utf8(bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

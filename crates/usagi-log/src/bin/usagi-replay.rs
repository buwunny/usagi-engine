//! Replays Tenhou logs through the engine and reports mismatches.
//!
//!     cargo run --release -p usagi-log --bin usagi-replay -- [--threads N] LOGS...
//!
//! Each argument is a log file (plain or gzipped), an archive of logs
//! (`.tar`, `.tar.gz`, `.tar.zst`), an SQLite database of logs, or a
//! directory searched recursively for any of those; see
//! [`usagi_log::source`]. Logs are replayed on one thread per core unless
//! `--threads` says otherwise. Logs with other rules (sanma, no red fives,
//! no kuitan) are skipped. Exits non-zero if any hand doesn't match.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use usagi_log::source::{self, Source};
use usagi_log::{Report, parse, replay};

enum Outcome {
    Unreadable(String),
    Skipped,
    Replayed(Report),
}

fn main() -> ExitCode {
    let Some((threads, sources)) = args() else {
        eprintln!("usage: usagi-replay [--threads N] LOG_OR_ARCHIVE_OR_DIR...");
        return ExitCode::FAILURE;
    };
    let start = Instant::now();
    let (mut games, mut hands, mut skipped, mut bad_games, mut bad_hands) = (0, 0, 0, 0, 0);
    source::for_each(
        &sources,
        threads,
        |_, text| {
            let game = match text.and_then(|s| parse(&s).map_err(|e| e.to_string())) {
                Ok(g) => g,
                Err(e) => return Outcome::Unreadable(e),
            };
            if !game.is_tenhou_ranked_rules() {
                return Outcome::Skipped;
            }
            Outcome::Replayed(replay(&game))
        },
        |(name, outcome)| match outcome {
            Outcome::Unreadable(e) => {
                println!("{name}: can't read: {e}");
                bad_games += 1;
            }
            Outcome::Skipped => skipped += 1,
            Outcome::Replayed(report) => {
                games += 1;
                hands += report.hands;
                if !report.ok() {
                    bad_games += 1;
                    bad_hands += report.mismatches.len();
                    for m in &report.mismatches {
                        println!("{name}: {m}");
                    }
                }
                if games % 10_000 == 0 {
                    eprintln!("{games} games replayed ({:.0?})", start.elapsed());
                }
            }
        },
    );
    println!(
        "{games} games, {hands} hands replayed in {:.1?}; {skipped} skipped (other rules); \
         {bad_games} games with problems, {bad_hands} mismatched hands",
        start.elapsed()
    );
    if bad_games == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// `--threads N` and the inputs, or `None` for a usage error.
fn args() -> Option<(usize, Vec<Source>)> {
    let mut threads = source::default_threads();
    let mut paths = Vec::new();
    let mut it = std::env::args_os().skip(1);
    while let Some(a) = it.next() {
        if a == "--threads" {
            threads = it.next()?.to_str()?.parse().ok()?;
        } else {
            paths.push(PathBuf::from(a));
        }
    }
    if paths.is_empty() {
        return None;
    }
    Some((threads, source::collect(&paths)))
}

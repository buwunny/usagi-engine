//! Replays Tenhou logs through the engine and reports mismatches.
//!
//!     cargo run --release -p usagi-log --bin usagi-replay -- [--threads N] [--save DIR] LOGS...
//!
//! Each argument is a log file (plain or gzipped), an archive of logs
//! (`.tar`, `.tar.gz`, `.tar.zst`), an SQLite database of logs, or a
//! directory searched recursively for any of those; see
//! [`usagi_log::source`]. Logs are replayed on one thread per core unless
//! `--threads` says otherwise. Logs with other rules (sanma, no red fives,
//! no kuitan) are skipped. `--save DIR` writes each log that doesn't match
//! (or can't be parsed) to `DIR/<id>.mjlog`, for a closer look. Logs from
//! before June 2010 are replayed with that era's game-end rule (see
//! [`usagi_log::Options`]). Hands that differ after a player disconnected
//! are counted apart (see [`usagi_log::Report::disconnected`]). Exits
//! non-zero if any other hand doesn't match.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use usagi_log::source::{self, Source};
use usagi_log::{Options, Report, parse, replay_with};

enum Outcome {
    Unreadable(String),
    Skipped,
    Replayed(Report),
}

fn main() -> ExitCode {
    let Some(Args {
        threads,
        save,
        sources,
    }) = args()
    else {
        eprintln!("usage: usagi-replay [--threads N] [--save DIR] LOG_OR_ARCHIVE_OR_DIR...");
        return ExitCode::FAILURE;
    };
    if let Some(dir) = &save
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        eprintln!("can't create {}: {e}", dir.display());
        return ExitCode::FAILURE;
    }
    let start = Instant::now();
    let (mut games, mut hands, mut skipped, mut bad_games, mut bad_hands) = (0, 0, 0, 0, 0);
    let mut disconnected = 0;
    source::for_each(
        &sources,
        threads,
        |name, text| {
            let text = match text {
                Ok(t) => t,
                Err(e) => return Outcome::Unreadable(e),
            };
            let outcome = match parse(&text) {
                Err(e) => Outcome::Unreadable(e.to_string()),
                Ok(game) if !game.is_tenhou_ranked_rules() => Outcome::Skipped,
                Ok(game) => {
                    Outcome::Replayed(replay_with(&game, Options::for_log_id(name), &mut |_| {}))
                }
            };
            let bad = match &outcome {
                Outcome::Unreadable(_) => true,
                Outcome::Replayed(r) => !r.ok(),
                Outcome::Skipped => false,
            };
            if bad && let Some(dir) = &save {
                let id = name.rsplit([':', '/']).next().unwrap_or(name);
                let id = id.trim_end_matches(".gz").trim_end_matches(".mjlog");
                let path = dir.join(format!("{id}.mjlog"));
                if let Err(e) = std::fs::write(&path, &text) {
                    eprintln!("can't write {}: {e}", path.display());
                }
            }
            outcome
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
                disconnected += report.disconnected.len();
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
    if disconnected > 0 {
        println!(
            "{disconnected} more hands differ after a player disconnected (not counted; \
             Tenhou's play for a disconnected player doesn't follow the rules)"
        );
    }
    if bad_games == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

struct Args {
    threads: usize,
    save: Option<PathBuf>,
    sources: Vec<Source>,
}

/// The parsed command line, or `None` for a usage error.
fn args() -> Option<Args> {
    let mut threads = source::default_threads();
    let mut save = None;
    let mut paths = Vec::new();
    let mut it = std::env::args_os().skip(1);
    while let Some(a) = it.next() {
        if a == "--threads" {
            threads = it.next()?.to_str()?.parse().ok()?;
        } else if a == "--save" {
            save = Some(PathBuf::from(it.next()?));
        } else {
            paths.push(PathBuf::from(a));
        }
    }
    if paths.is_empty() {
        return None;
    }
    Some(Args {
        threads,
        save,
        sources: source::collect(&paths),
    })
}

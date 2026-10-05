//! Replays Tenhou logs through the usagi engine and riichienv-core side by
//! side and compares the legal actions at every decision point.
//!
//!     cargo run --release -p usagi-log --example compare-riichienv -- [--threads N] LOGS...
//!
//! Arguments are log files, archives, databases or directories, as for
//! `usagi-replay`, and `--threads N` (default: one per core). Prints every
//! difference, then a summary grouped by kind. Exits non-zero if anything
//! differs.

#[path = "../tests/oracle/mod.rs"]
mod oracle;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use oracle::{Comparison, Oracle};
use usagi_log::source::{self, Source};
use usagi_log::{Options, parse, replay_with};

enum Outcome {
    Unreadable(String),
    Skipped,
    Compared(Comparison, usize),
}

fn main() -> ExitCode {
    let Some((threads, sources)) = args() else {
        eprintln!("usage: compare-riichienv [--threads N] LOG_OR_ARCHIVE_OR_DIR...");
        return ExitCode::FAILURE;
    };
    let start = Instant::now();
    let (mut games, mut points, mut hands, mut cut, mut replay_bad, mut unreadable) =
        (0, 0, 0, 0, 0, 0);
    let mut by_kind: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut diffs = 0;
    source::for_each(
        &sources,
        threads,
        |name, text| {
            let game = match text.and_then(|s| parse(&s).map_err(|e| e.to_string())) {
                Ok(g) => g,
                Err(e) => return Outcome::Unreadable(e),
            };
            if !game.is_tenhou_ranked_rules() {
                return Outcome::Skipped;
            }
            let mut o = Oracle::new();
            let report = replay_with(&game, Options::for_log_id(name), &mut |e| o.on_event(e));
            Outcome::Compared(o.result, report.mismatches.len())
        },
        |(name, outcome)| {
            let (r, bad) = match outcome {
                Outcome::Unreadable(e) => {
                    println!("{name}: can't read: {e}");
                    unreadable += 1;
                    return;
                }
                Outcome::Skipped => return,
                Outcome::Compared(r, bad) => (r, bad),
            };
            games += 1;
            replay_bad += bad;
            points += r.points;
            hands += r.hands;
            cut += r.hands_cut_short;
            for d in &r.diffs {
                diffs += 1;
                println!("{name}: {d}");
                let e = by_kind
                    .entry(d.category())
                    .or_insert((0, format!("{name}: {d}")));
                e.0 += 1;
            }
            for why in &r.cut_reasons {
                println!("{name}: riichienv stopped: {why}");
            }
            if games % 10_000 == 0 {
                eprintln!("{games} games compared ({:.0?})", start.elapsed());
            }
        },
    );
    println!();
    println!(
        "{games} games, {hands} hands, {points} decision points compared in {:.1?}",
        start.elapsed()
    );
    println!(
        "{diffs} differences; {cut} hands riichienv couldn't finish; \
         {replay_bad} usagi replay mismatches; {unreadable} unreadable logs"
    );
    for (kind, (n, example)) in &by_kind {
        println!("  {n:6}  {kind}\n          e.g. {example}");
    }
    if diffs == 0 && cut == 0 && unreadable == 0 {
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

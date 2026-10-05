//! Replays Tenhou logs through the usagi engine and riichienv-core side by
//! side and compares the legal actions at every decision point.
//!
//!     cargo run --release -p usagi-log --example compare-riichienv -- LOGS...
//!
//! Arguments are mjlog files or directories, as for `usagi-replay`. Prints
//! every difference, then a summary grouped by kind. Exits non-zero if
//! anything differs.

#[path = "../tests/oracle/mod.rs"]
mod oracle;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use oracle::Oracle;
use usagi_log::{parse, read_log, replay_with};

fn main() -> ExitCode {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.is_empty() {
        eprintln!("usage: compare-riichienv LOG_OR_DIR...");
        return ExitCode::FAILURE;
    }
    let mut files = Vec::new();
    for a in &args {
        collect(a, &mut files);
    }
    files.sort();

    let start = Instant::now();
    let (mut games, mut points, mut hands, mut cut, mut replay_bad) = (0, 0, 0, 0, 0);
    let mut by_kind: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut diffs = 0;
    for path in &files {
        let Ok(game) = read_log(path)
            .map_err(|e| e.to_string())
            .and_then(|s| parse(&s).map_err(|e| e.to_string()))
        else {
            println!("{}: parse error", path.display());
            continue;
        };
        if !game.is_tenhou_ranked_rules() {
            continue;
        }
        let mut o = Oracle::new();
        let report = replay_with(&game, &mut |e| o.on_event(e));
        games += 1;
        replay_bad += report.mismatches.len();
        let r = o.result;
        points += r.points;
        hands += r.hands;
        cut += r.hands_cut_short;
        let name = path.file_name().unwrap().to_string_lossy();
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
    }
    println!();
    println!(
        "{games} games, {hands} hands, {points} decision points compared in {:.1?}",
        start.elapsed()
    );
    println!(
        "{diffs} differences; {cut} hands riichienv couldn't finish; \
         {replay_bad} usagi replay mismatches"
    );
    for (kind, (n, example)) in &by_kind {
        println!("  {n:6}  {kind}\n          e.g. {example}");
    }
    if diffs == 0 && cut == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let Ok(entries) = std::fs::read_dir(path) else {
            eprintln!("can't read {}", path.display());
            return;
        };
        for e in entries.flatten() {
            collect(&e.path(), out);
        }
    } else if path.is_file()
        && matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("mjlog" | "xml" | "gz")
        )
    {
        out.push(path.to_path_buf());
    }
}

//! Replays Tenhou logs through the engine and reports mismatches.
//!
//!     cargo run --release -p usagi-log --bin usagi-replay -- LOGS...
//!
//! Each argument is an mjlog file (plain or gzipped) or a directory,
//! searched recursively for `.mjlog`, `.xml` and `.gz` files. Logs with
//! other rules (sanma, no red fives, no kuitan) are skipped. Exits non-zero
//! if any hand doesn't match.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use usagi_log::{parse, read_log, replay};

fn main() -> ExitCode {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if args.is_empty() {
        eprintln!("usage: usagi-replay LOG_OR_DIR...");
        return ExitCode::FAILURE;
    }
    let mut files = Vec::new();
    for a in &args {
        collect(a, &mut files);
    }
    files.sort();

    let start = Instant::now();
    let (mut games, mut hands, mut skipped, mut bad_games, mut bad_hands) = (0, 0, 0, 0, 0);
    for path in &files {
        let game = match read_log(path)
            .map_err(|e| e.to_string())
            .and_then(|s| parse(&s).map_err(|e| e.to_string()))
        {
            Ok(g) => g,
            Err(e) => {
                println!("{}: parse error: {e}", path.display());
                bad_games += 1;
                continue;
            }
        };
        if !game.is_tenhou_ranked_rules() {
            skipped += 1;
            continue;
        }
        let report = replay(&game);
        games += 1;
        hands += report.hands;
        if !report.ok() {
            bad_games += 1;
            bad_hands += report.mismatches.len();
            for m in &report.mismatches {
                println!("{}: {m}", path.display());
            }
        }
    }
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

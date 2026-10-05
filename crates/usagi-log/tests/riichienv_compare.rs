//! The usagi engine and riichienv-core offer the same legal actions at
//! every decision point of the sample logs. Run the same check over a
//! full log archive with `examples/compare-riichienv.rs`.

mod oracle;

use std::path::{Path, PathBuf};

use oracle::Oracle;
use usagi_log::{ReplayEvent, parse, read_log, replay_with};

fn sample_logs() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut out = Vec::new();
    let mut stack = vec![dir];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("mjlog" | "xml" | "gz")
            ) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn sample_logs_match_riichienv() {
    let (mut points, mut failures) = (0, Vec::new());
    for path in sample_logs() {
        let game = parse(&read_log(&path).unwrap()).unwrap();
        let mut o = Oracle::new();
        replay_with(&game, &mut |e| o.on_event(e));
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        points += o.result.points;
        failures.extend(o.result.diffs.iter().map(|d| format!("{name}: {d}")));
        failures.extend(
            o.result
                .cut_reasons
                .iter()
                .map(|w| format!("{name}: riichienv stopped: {w}")),
        );
    }
    println!("{points} decision points compared");
    assert!(points > 10_000, "only {points} decision points compared");
    assert!(
        failures.is_empty(),
        "{} differences:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Giving riichienv a different wall from the engine's must show up as
/// differences, so a silent harness can't pass for agreement.
#[test]
fn a_different_wall_is_noticed() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/mjx/2010091009gm-00a9-0000-83af2648.mjlog");
    let game = parse(&read_log(&path).unwrap()).unwrap();
    let mut o = Oracle::new();
    replay_with(&game, &mut |e| match e {
        ReplayEvent::Deal {
            hand,
            init,
            wall,
            state,
        } => {
            // Swap the dealer's first tile with the next seat's first
            // tile of another kind.
            let mut w = *wall;
            let j = (13..26).find(|&j| w[j] / 4 != w[0] / 4).unwrap();
            w.swap(0, j);
            o.on_event(ReplayEvent::Deal {
                hand,
                init,
                wall: &w,
                state,
            });
        }
        e => o.on_event(e),
    });
    assert!(
        o.result.diffs.len() as u64 + o.result.hands_cut_short >= o.result.hands,
        "every hand should differ: {} hands, {} diffs, {} cut short",
        o.result.hands,
        o.result.diffs.len(),
        o.result.hands_cut_short
    );
}

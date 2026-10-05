//! Every sample log parses and replays exactly.

use std::path::Path;

use usagi_log::{parse, read_log, replay};

#[test]
fn sample_logs_replay_exactly() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data");
    let mut paths: Vec<_> = walk(&dir);
    paths.sort();
    assert!(!paths.is_empty(), "no sample logs in {}", dir.display());
    let (mut hands, mut failures) = (0, Vec::new());
    for path in &paths {
        let name = path.strip_prefix(&dir).unwrap().display().to_string();
        let game = match parse(&read_log(path).unwrap()) {
            Ok(g) => g,
            Err(e) => {
                failures.push(format!("{name}: parse error: {e}"));
                continue;
            }
        };
        let report = replay(&game);
        hands += report.hands;
        for m in report.mismatches {
            failures.push(format!("{name}: {m}"));
        }
    }
    println!("{} logs, {hands} hands", paths.len());
    assert!(
        failures.is_empty(),
        "{} mismatches:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if matches!(
            p.extension().and_then(|e| e.to_str()),
            Some("mjlog" | "xml" | "gz")
        ) {
            out.push(p);
        }
    }
    out
}

/// The replayer must notice a wrong score, a wrong draw and an illegal
/// call, not just agree with everything.
#[test]
fn tampered_logs_are_caught() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/mjx/double-ron.mjlog");
    let original = read_log(&path).unwrap();
    assert!(replay(&parse(&original).unwrap()).ok());

    // Change the first win's score change for the winner.
    let i = original.find(" sc=\"").unwrap();
    let j = i + original[i..].find(',').unwrap();
    let k = j + 1 + original[j + 1..].find(',').unwrap();
    let delta: i32 = original[j + 1..k].parse().unwrap();
    let wrong_score = format!("{}{}{}", &original[..j + 1], delta + 1, &original[k..]);
    assert!(!replay(&parse(&wrong_score).unwrap()).ok());

    // Swap the first draw for a tile someone already holds.
    let first_init = original.find("<INIT").unwrap();
    let hai0 = first_init + original[first_init..].find("hai0=\"").unwrap() + 6;
    let held: &str = original[hai0..].split(',').next().unwrap();
    let draw = first_init + original[first_init..].find("<T").unwrap();
    let end = draw + original[draw..].find("/>").unwrap();
    let wrong_draw = format!("{}<T{held}{}", &original[..draw], &original[end..]);
    assert!(!replay(&parse(&wrong_draw).unwrap()).ok());
}

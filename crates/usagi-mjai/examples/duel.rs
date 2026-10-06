//! One bot level against three of another, on every core.
//!
//!     cargo run --release -p usagi-mjai --example duel -- HERO OTHERS [GAMES]
//!
//! The hero sits in each seat equally often. Prints its average rank and
//! final score.

use usagi_mjai::{Bot, Level, RuleBot, play_game};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let hero = Level::from_name(&args[0]).expect("hero level");
    let others = Level::from_name(&args[1]).expect("other level");
    let games: u64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(800);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get()) as u64;
    let results: Vec<(u64, i64)> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..threads)
            .map(|t| {
                s.spawn(move || {
                    let (mut rank, mut score) = (0u64, 0i64);
                    for g in (t..games).step_by(threads as usize) {
                        let seat = (g % 4) as usize;
                        let mut bots: Vec<RuleBot> = (0..4)
                            .map(|s| {
                                RuleBot::new(
                                    if s == seat { hero } else { others },
                                    g * 4 + s as u64,
                                )
                            })
                            .collect();
                        let [b0, b1, b2, b3] = &mut bots[..] else {
                            unreachable!()
                        };
                        let rec = play_game(g / 4, [b0 as &mut dyn Bot, b1, b2, b3])
                            .unwrap_or_else(|e| panic!("game {g}: {e}"));
                        let me = rec.scores[seat];
                        rank += 1
                            + (0..4)
                                .filter(|&s| rec.scores[s] > me || rec.scores[s] == me && s < seat)
                                .count() as u64;
                        score += me as i64;
                    }
                    (rank, score)
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let rank: u64 = results.iter().map(|r| r.0).sum();
    let score: i64 = results.iter().map(|r| r.1).sum();
    println!(
        "one {hero} vs three {others}, {games} games: average rank {:.3}, average score {:.0}",
        rank as f64 / games as f64,
        score as f64 / games as f64
    );
}

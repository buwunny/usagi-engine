//! Random-play games per second on every core.
//!
//!     cargo run --release -p usagi-engine --example throughput [SECONDS] [THREADS]

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use usagi_engine::{GameState, Phase};

fn random_game(seed: u64) -> u64 {
    let mut g: GameState = GameState::new(seed);
    let mut events = Vec::with_capacity(16);
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut decisions = 0;
    loop {
        match g.phase {
            Phase::GameEnd => return decisions,
            Phase::RoundEnd => g.start_next_round(&mut events).unwrap(),
            _ => {
                let seat = g.waiting_on().trailing_zeros() as u8;
                let legal = g.legal_actions(seat);
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                let a = legal.as_slice()[(x % legal.len() as u64) as usize];
                g.step(seat, a, &mut events).unwrap();
                decisions += 1;
            }
        }
        events.clear();
    }
}

fn main() {
    let args: Vec<u64> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let run_for = Duration::from_secs(args.first().copied().unwrap_or(5));
    let threads = args.get(1).map_or_else(
        || std::thread::available_parallelism().map_or(1, |n| n.get()),
        |&t| t as usize,
    );
    let games = AtomicU64::new(0);
    let decisions = AtomicU64::new(0);
    let next_seed = AtomicU64::new(0);
    let start = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                while start.elapsed() < run_for {
                    let seed = next_seed.fetch_add(1, Ordering::Relaxed);
                    decisions.fetch_add(random_game(seed), Ordering::Relaxed);
                    games.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });
    let secs = start.elapsed().as_secs_f64();
    let g = games.load(Ordering::Relaxed) as f64;
    let d = decisions.load(Ordering::Relaxed) as f64;
    println!(
        "{threads} threads: {:.0} games/s, {:.0} decisions/s ({:.0} decisions per game)",
        g / secs,
        d / secs,
        d / g
    );
}

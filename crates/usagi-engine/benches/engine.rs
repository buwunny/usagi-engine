//! Whole games with random legal actions.
//!
//!     cargo bench -p usagi-engine

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use usagi_engine::{GameState, Phase};

/// Plays one game where each decision is a pseudo-random legal action.
pub fn random_game(seed: u64) -> [i32; 4] {
    let mut g: GameState = GameState::new(seed);
    let mut events = Vec::with_capacity(16);
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    loop {
        match g.phase {
            Phase::GameEnd => return g.scores,
            Phase::RoundEnd => {
                g.start_next_round(&mut events).unwrap();
            }
            _ => {
                let seat = g.waiting_on().trailing_zeros() as u8;
                let legal = g.legal_actions(seat);
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                let a = legal.as_slice()[(x % legal.len() as u64) as usize];
                g.step(seat, a, &mut events).unwrap();
            }
        }
        events.clear();
    }
}

fn bench_games(c: &mut Criterion) {
    let mut seed = 0;
    c.bench_function("random game", |b| {
        b.iter(|| {
            seed += 1;
            random_game(black_box(seed))
        })
    });
    c.bench_function("copy GameState", |b| {
        let g: GameState = GameState::new(1);
        b.iter(|| black_box(*black_box(&g)))
    });
}

criterion_group!(benches, bench_games);
criterion_main!(benches);

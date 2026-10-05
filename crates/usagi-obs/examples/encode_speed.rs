//! Times `encode_into` over the states of random games.
//!
//!     cargo run --release -p usagi-obs --example encode_speed

use std::hint::black_box;
use std::time::Instant;

use usagi_engine::{GameState, Phase};
use usagi_obs::{Observation, encode_into};

fn main() {
    let mut states: Vec<GameState> = Vec::new();
    for seed in 0..20 {
        let mut g: GameState = GameState::new(seed);
        let mut events = Vec::new();
        let mut i = seed as usize;
        while g.phase != Phase::GameEnd {
            if g.phase == Phase::RoundEnd {
                g.start_next_round(&mut events).unwrap();
                continue;
            }
            states.push(g);
            let seat = g.waiting_on().trailing_zeros() as u8;
            let legal = g.legal_actions(seat);
            i = i.wrapping_mul(31).wrapping_add(7);
            g.step(seat, legal[i % legal.len()], &mut events).unwrap();
        }
    }
    let mut obs = Observation::default();
    let start = Instant::now();
    let rounds = 20;
    for _ in 0..rounds {
        for g in &states {
            for seat in 0..4 {
                encode_into(black_box(g), seat, &mut obs);
                black_box(&obs);
            }
        }
    }
    let n = rounds * states.len() * 4;
    println!(
        "{n} encodes, {:.2} µs each",
        start.elapsed().as_secs_f64() * 1e6 / n as f64
    );
}

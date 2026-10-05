//! Plays one game where every seat takes its first legal action.
//!
//!     cargo run --release -p usagi-engine --example first_legal

use usagi_engine::{GameState, Phase};

fn main() {
    let mut game: GameState = GameState::new(42);
    let mut events = Vec::new();
    while game.phase != Phase::GameEnd {
        if game.phase == Phase::RoundEnd {
            game.start_next_round(&mut events).unwrap();
            continue;
        }
        let seat = game.waiting_on().trailing_zeros() as u8;
        let action = game.legal_actions(seat)[0];
        game.step(seat, action, &mut events).unwrap();
    }
    println!("{} events, final scores {:?}", events.len(), game.scores);
}

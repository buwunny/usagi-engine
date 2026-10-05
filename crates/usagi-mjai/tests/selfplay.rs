//! Baseline bots play whole games at the table without an illegal move.

use usagi_mjai::convert::mask;
use usagi_mjai::{Baseline, Bot, Event, Pai, ProcessBot, play_game};

#[test]
fn baseline_bots_finish_games() {
    let (mut wins, mut riichi, mut ryukyoku) = (0, 0, 0);
    for seed in 0..60 {
        let mut b = [
            Baseline::new(),
            Baseline::new(),
            Baseline::new(),
            Baseline::new(),
        ];
        let [b0, b1, b2, b3] = &mut b;
        let record = play_game(seed, [b0, b1, b2, b3]).unwrap_or_else(|e| {
            panic!(
                "seed {seed}: {e}\nlast events: {:?}",
                &e.log[e.log.len().saturating_sub(12)..]
            )
        });
        assert_eq!(record.scores.iter().sum::<i32>() % 1000, 0);
        assert!(matches!(record.log.last(), Some(Event::EndGame)));
        for e in &record.log {
            match e {
                Event::Hora { .. } => wins += 1,
                Event::ReachAccepted { .. } => riichi += 1,
                Event::Ryukyoku { .. } => ryukyoku += 1,
                _ => {}
            }
        }
    }
    println!("wins {wins}, riichi {riichi}, draws {ryukyoku}");
    assert!(wins > 200 && riichi > 200, "wins {wins}, riichi {riichi}");
}

#[test]
fn bots_in_child_processes_finish_a_game() {
    let cmd = format!("{} bot", env!("CARGO_BIN_EXE_usagi-mjai"));
    let mut bots: Vec<ProcessBot> = (0..4).map(|_| ProcessBot::spawn(&cmd).unwrap()).collect();
    let [b0, b1, b2, b3] = &mut bots[..] else {
        unreachable!()
    };
    let mut local = [
        Baseline::new(),
        Baseline::new(),
        Baseline::new(),
        Baseline::new(),
    ];
    let [l0, l1, l2, l3] = &mut local;
    // Same seed, same bot logic: the processes must play the same game.
    let remote = play_game(3, [b0 as &mut dyn Bot, b1, b2, b3]).unwrap();
    let inproc = play_game(3, [l0 as &mut dyn Bot, l1, l2, l3]).unwrap();
    assert_eq!(remote.scores, inproc.scores);
    assert_eq!(remote.log, inproc.log);
}

#[test]
fn masking_hides_other_seats_tiles() {
    let tsumo = Event::Tsumo {
        actor: 2,
        pai: "5mr".parse().unwrap(),
    };
    assert_eq!(
        mask(1, &tsumo),
        Event::Tsumo {
            actor: 2,
            pai: Pai::UNKNOWN
        }
    );
    assert_eq!(mask(2, &tsumo), tsumo);
}

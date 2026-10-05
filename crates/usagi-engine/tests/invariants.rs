//! Whole-game invariants, checked after every step of random games in
//! which every seat picks a uniformly random legal action.

use usagi_engine::state::{GameState, discard_bits};
use usagi_engine::{Event, Phase, Rules, TenhouRules};

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// Plays one random game, calling `check` after every step and every deal.
/// Returns all events.
fn play_random_game(seed: u64, mut check: impl FnMut(&GameState)) -> Vec<Event> {
    let mut game: GameState = GameState::new(seed);
    let mut rng = Lcg(seed ^ 0xDEAD_BEEF);
    let mut events = Vec::new();
    let mut steps = 0;
    check(&game);
    while !game.is_over() {
        if game.phase == Phase::RoundEnd {
            game.start_next_round(&mut events).unwrap();
            check(&game);
            continue;
        }
        let waiting = game.waiting_on();
        assert_ne!(
            waiting, 0,
            "seed {seed}: waiting on nobody in {:?}",
            game.phase
        );
        let seat = waiting.trailing_zeros() as u8;
        let actions = game.legal_actions(seat);
        assert!(
            !actions.is_empty(),
            "seed {seed}: seat {seat} has no action"
        );
        let pick = actions.as_slice()[(rng.next() % actions.len() as u64) as usize];
        game.step(seat, pick, &mut events)
            .unwrap_or_else(|e| panic!("seed {seed}: legal {pick:?} refused: {e:?}"));
        check(&game);
        steps += 1;
        assert!(steps < 200_000, "seed {seed}: game is not ending");
    }
    events
}

/// Where every one of the 136 tiles currently is.
fn count_tiles(g: &GameState) -> usize {
    let live_left = g.tiles_left() as usize;
    let dead = 14;
    let mut n = live_left + dead;
    for p in &g.players {
        n += p.concealed_count() as usize;
        n += p
            .discards()
            .iter()
            .filter(|&&d| d & discard_bits::CALLED == 0)
            .count();
        for m in p.melds() {
            n += if m.kind().is_kan() { 4 } else { 3 };
        }
    }
    // Rinshan draws come from the dead wall, which is refilled from the
    // live wall: `tiles_left` already accounts for that.
    n
}

const GAMES: u64 = 300;

#[test]
fn new_game_deals_correctly() {
    let game: GameState = GameState::new(1);
    let held: Vec<u32> = game.players.iter().map(|p| p.concealed_count()).collect();
    let dealer = game.dealer() as usize;
    for (s, &h) in held.iter().enumerate() {
        assert_eq!(h, if s == dealer { 14 } else { 13 });
    }
    assert_eq!(game.scores, [25_000; 4]);
    assert_eq!(game.tiles_left(), 69);
    assert_eq!(game.waiting_on(), 1 << dealer);
}

#[test]
fn points_are_conserved() {
    for seed in 0..GAMES {
        play_random_game(seed, |g| {
            let total: i32 = g.scores.iter().sum::<i32>() + 1000 * g.riichi_sticks as i32;
            assert_eq!(total, 4 * TenhouRules::STARTING_POINTS, "seed {seed}");
        });
    }
}

#[test]
fn every_tile_is_somewhere() {
    for seed in 0..GAMES {
        play_random_game(seed, |g| {
            if !matches!(g.phase, Phase::RoundEnd | Phase::GameEnd) {
                assert_eq!(count_tiles(g), 136, "seed {seed} phase {:?}", g.phase);
            }
        });
    }
}

#[test]
fn hand_sizes_stay_valid() {
    for seed in 0..GAMES {
        play_random_game(seed, |g| {
            for (s, p) in g.players.iter().enumerate() {
                let n = p.concealed_count() + 3 * p.meld_count as u32;
                assert!(n == 13 || n == 14, "seed {seed} seat {s}: {n} tiles");
            }
        });
    }
}

#[test]
fn same_seed_same_game() {
    for seed in 0..20 {
        assert_eq!(
            play_random_game(seed, |_| {}),
            play_random_game(seed, |_| {})
        );
    }
}

#[test]
fn games_end_and_cover_many_outcomes() {
    let mut wins = 0;
    let mut draws = 0;
    let mut aborts = 0;
    let mut calls = 0;
    let mut kans = 0;
    let mut riichi = 0;
    for seed in 0..GAMES {
        for e in play_random_game(seed, |_| {}) {
            match e {
                Event::Win { .. } => wins += 1,
                Event::ExhaustiveDraw { .. } => draws += 1,
                Event::AbortiveDraw { .. } => aborts += 1,
                Event::Call { .. } => calls += 1,
                Event::Kan { .. } => kans += 1,
                Event::RiichiAccepted { .. } => riichi += 1,
                _ => {}
            }
        }
    }
    println!("wins {wins} draws {draws} aborts {aborts} calls {calls} kans {kans} riichi {riichi}");
    assert!(wins > 0 && draws > 0 && calls > 0 && kans > 0 && riichi > 0);
}

/// A policy that plays roughly like a beginner: always wins when it can,
/// often declares riichi, sometimes calls, and otherwise discards the
/// tile that leaves the lowest shanten. Reaches far more wins, riichi and
/// furiten situations than uniform random play.
fn greedy_pick(g: &GameState, seat: u8, rng: &mut Lcg) -> usagi_engine::Action {
    use usagi_core::Counts;
    use usagi_core::shanten::shanten;
    use usagi_engine::Action;
    let actions = g.legal_actions(seat);
    let list = actions.as_slice();
    if let Some(&a) = list
        .iter()
        .find(|a| matches!(a, Action::Tsumo | Action::Ron))
    {
        if rng.next() % 10 != 0 {
            return a;
        }
    }
    let p = &g.players[seat as usize];
    let after = |k: u8| {
        let mut c = Counts(p.hand);
        c.0[k as usize] -= 1;
        shanten(&c, p.meld_count)
    };
    let best_discard = list
        .iter()
        .filter_map(|a| match a {
            Action::Discard(t) | Action::Riichi(t) => Some((after(t.kind()), *a)),
            _ => None,
        })
        .min_by_key(|(s, a)| (*s, !matches!(a, Action::Riichi(_))));
    if let Some((_, a)) = best_discard {
        if rng.next() % 4 != 0 {
            return a;
        }
    }
    if list
        .iter()
        .any(|a| matches!(a, Action::Pon(_) | Action::Chi(_) | Action::Daiminkan))
        && rng.next() % 3 == 0
    {
        let calls: Vec<_> = list
            .iter()
            .filter(|a| !matches!(a, Action::Pass | Action::Ron))
            .collect();
        return *calls[(rng.next() % calls.len() as u64) as usize];
    }
    if list.contains(&Action::Pass) {
        return Action::Pass;
    }
    list[(rng.next() % list.len() as u64) as usize]
}

fn play_greedy_game(seed: u64, mut check: impl FnMut(&GameState)) -> Vec<Event> {
    let mut game: GameState = GameState::new(seed);
    let mut rng = Lcg(seed ^ 0x1234_5678);
    let mut events = Vec::new();
    let mut steps = 0;
    while !game.is_over() {
        if game.phase == Phase::RoundEnd {
            game.start_next_round(&mut events).unwrap();
            continue;
        }
        let seat = game.waiting_on().trailing_zeros() as u8;
        let pick = greedy_pick(&game, seat, &mut rng);
        game.step(seat, pick, &mut events)
            .unwrap_or_else(|e| panic!("seed {seed}: legal {pick:?} refused: {e:?}"));
        check(&game);
        steps += 1;
        assert!(steps < 200_000, "seed {seed}: game is not ending");
    }
    events
}

#[test]
fn greedy_games_keep_invariants() {
    let (mut wins, mut tsumo, mut double_ron, mut riichi, mut draws, mut aborts) =
        (0, 0, 0, 0, 0, 0);
    for seed in 0..GAMES {
        let events = play_greedy_game(seed, |g| {
            let total: i32 = g.scores.iter().sum::<i32>() + 1000 * g.riichi_sticks as i32;
            assert_eq!(total, 100_000, "seed {seed}");
            if !matches!(g.phase, Phase::RoundEnd | Phase::GameEnd) {
                assert_eq!(count_tiles(g), 136, "seed {seed}");
            }
        });
        let mut last_win_from = None;
        for e in events {
            match e {
                Event::Win { from, .. } => {
                    wins += 1;
                    if from == u8::MAX {
                        tsumo += 1;
                    } else if last_win_from == Some(from) {
                        double_ron += 1;
                    }
                    last_win_from = Some(from);
                }
                Event::RoundEnded { .. } => last_win_from = None,
                Event::RiichiAccepted { .. } => riichi += 1,
                Event::ExhaustiveDraw { .. } => draws += 1,
                Event::AbortiveDraw { .. } => aborts += 1,
                _ => {}
            }
        }
    }
    println!(
        "wins {wins} (tsumo {tsumo}, double ron {double_ron}) riichi {riichi} draws {draws} aborts {aborts}"
    );
    assert!(wins > 300 && tsumo > 0 && riichi > 0);
}

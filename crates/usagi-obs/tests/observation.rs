//! The observation hides what a seat can't know, and the action space is
//! a lossless numbering of legal actions.

use usagi_core::Tile;
use usagi_engine::state::NO_TILE;
use usagi_engine::wall::{DORA_START, LIVE_WALL_END, RINSHAN_START, WALL_SIZE};
use usagi_engine::{Action, GameState, Phase};
use usagi_obs::{NUM_ACTIONS, action_at, encode, index_of, legal_mask};

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Calls `visit` on every state of `games` random games.
fn for_each_state(games: u64, mut visit: impl FnMut(&GameState, &mut Lcg)) {
    for seed in 0..games {
        let mut rng = Lcg(seed);
        let mut g: GameState = GameState::new(seed);
        let mut events = Vec::new();
        while g.phase != Phase::GameEnd {
            if g.phase == Phase::RoundEnd {
                g.start_next_round(&mut events).unwrap();
                continue;
            }
            visit(&g, &mut rng);
            let waiting = g.waiting_on();
            let seat = waiting.trailing_zeros() as u8;
            let legal = g.legal_actions(seat);
            // Prefer winning so hands end in many different ways.
            let a = if legal.contains(&Action::Tsumo) {
                Action::Tsumo
            } else if legal.contains(&Action::Ron) {
                Action::Ron
            } else {
                legal[rng.below(legal.len())]
            };
            g.step(seat, a, &mut events).unwrap();
        }
    }
}

/// `g` with every tile `seat` can't see shuffled: the other seats'
/// concealed hands and the unseen part of the wall.
fn shuffle_hidden(g: &GameState, seat: u8, rng: &mut Lcg) -> GameState {
    let mut h = *g;
    let others: Vec<u8> = (1..4).map(|i| (seat + i) % 4).collect();
    let mut hidden: Vec<Tile> = Vec::new();
    let mut sizes = Vec::new();
    for &s in &others {
        let tiles = g.concealed_tiles(s);
        sizes.push(tiles.len());
        hidden.extend(tiles);
    }
    let wall_slots: Vec<usize> = (g.wall_pos as usize..LIVE_WALL_END as usize)
        .chain((RINSHAN_START + g.dead_pos) as usize..DORA_START as usize)
        .chain((DORA_START + g.dora_revealed) as usize..WALL_SIZE)
        .collect();
    hidden.extend(wall_slots.iter().map(|&i| g.wall[i]));
    for i in (1..hidden.len()).rev() {
        hidden.swap(i, rng.below(i + 1));
    }
    let mut it = hidden.into_iter();
    for (&s, &n) in others.iter().zip(&sizes) {
        let p = &mut h.players[s as usize];
        p.hand = [0; 34];
        p.reds = 0;
        let mut last = NO_TILE;
        for t in it.by_ref().take(n) {
            p.hand[t.kind() as usize] += 1;
            if t.is_red() {
                p.reds |= 1 << (t.kind() / 9);
            }
            last = t.code();
        }
        if p.drawn != NO_TILE {
            p.drawn = last;
        }
    }
    for i in wall_slots {
        h.wall[i] = it.next().unwrap();
    }
    h
}

#[test]
fn hidden_tiles_never_change_an_observation() {
    let mut states = 0;
    for_each_state(40, |g, rng| {
        for seat in 0..4 {
            let h = shuffle_hidden(g, seat, rng);
            assert_eq!(
                encode(g, seat),
                encode(&h, seat),
                "seat {seat}'s observation changed when only hidden tiles moved; phase {:?}",
                g.phase
            );
        }
        states += 1;
    });
    assert!(states > 10_000, "only {states} states");
}

#[test]
fn own_tiles_do_change_the_observation() {
    let g: GameState = GameState::new(7);
    let mut h = g;
    let p = &mut h.players[0];
    let from = p.hand.iter().position(|&n| n > 0).unwrap();
    let to = p.hand.iter().position(|&n| n == 0).unwrap();
    p.hand[from] -= 1;
    p.hand[to] += 1;
    assert_ne!(encode(&g, 0), encode(&h, 0));
    // ...and seat 1 can't tell.
    assert_eq!(encode(&g, 1), encode(&h, 1));
}

#[test]
fn legal_actions_get_distinct_indices_that_round_trip() {
    for_each_state(40, |g, _| {
        for seat in 0..4 {
            let legal = g.legal_actions(seat);
            let mut seen = [false; NUM_ACTIONS];
            for &a in legal.iter() {
                let i = index_of(g, a).unwrap_or_else(|| panic!("{a:?} has no index"));
                assert!(
                    !seen[i],
                    "two legal actions share index {i}: {:?}",
                    &legal[..]
                );
                seen[i] = true;
                assert_eq!(action_at(g, seat, i), Some(a));
            }
            let mask = legal_mask(g, seat);
            assert_eq!(mask.iter().map(|&m| m as usize).sum::<usize>(), legal.len());
        }
    });
}

//! Hand-built situations for rules that random play rarely reaches.

use mochitsuki_core::{Tile, tiles_of};
use mochitsuki_engine::state::GameState;
use mochitsuki_engine::wall::{WALL_SIZE, fresh_set};
use mochitsuki_engine::{AbortKind, Action, Event, Phase};

/// The dead wall used by every scenario: rinshan 1z x4, dora indicators
/// 2z (dora 3z), ura indicators 3z and 9s (ura 4z, 1s). None of the
/// winning hands below hold 3z, 4z or 1s, so no dora interferes.
const DEAD: &str = "11112222333z999s";

/// Builds a wall where seat `i` (East 1, seat 0 deals) starts with
/// `hands[i]`, the live wall continues with `draws` (the dealer's first
/// draw first), and the dead wall is [`DEAD`]. Unspecified slots get the
/// leftover tiles in order.
fn wall(hands: [&str; 4], draws: &str) -> [Tile; WALL_SIZE] {
    let mut left: Vec<Tile> = fresh_set(3).to_vec();
    let mut take = |t: Tile| {
        let i = left
            .iter()
            .position(|&x| x == t)
            .unwrap_or_else(|| panic!("no {t} left"));
        left.remove(i)
    };
    let mut slots: Vec<Option<Tile>> = vec![None; WALL_SIZE];
    for (seat, h) in hands.iter().enumerate() {
        let tiles = tiles_of(h);
        assert!(tiles.len() <= 13, "hand {seat} too long");
        for (i, t) in tiles.into_iter().enumerate() {
            slots[seat * 13 + i] = Some(take(t));
        }
    }
    for (i, t) in tiles_of(draws).into_iter().enumerate() {
        slots[52 + i] = Some(take(t));
    }
    let dead = tiles_of(DEAD);
    assert_eq!(dead.len(), 14);
    for (i, t) in dead.into_iter().enumerate() {
        slots[122 + i] = Some(take(t));
    }
    let mut rest = left.into_iter();
    let mut out = [Tile::from_kind(0); WALL_SIZE];
    for (o, s) in out.iter_mut().zip(slots) {
        *o = s.unwrap_or_else(|| rest.next().unwrap());
    }
    out
}

fn t(s: &str) -> Tile {
    tiles_of(s)[0]
}

fn game(hands: [&str; 4], draws: &str) -> GameState {
    GameState::with_wall(1, wall(hands, draws))
}

fn act(g: &mut GameState, seat: u8, a: Action) -> Vec<Event> {
    let mut ev = Vec::new();
    g.step(seat, a, &mut ev).unwrap_or_else(|e| {
        panic!(
            "seat {seat} {a:?}: {e:?}; legal: {:?}",
            g.legal_actions(seat).as_slice()
        )
    });
    ev
}

/// In a call or chankan window, every waiting seat except `keep` passes.
fn others_pass(g: &mut GameState, keep: u8) {
    while let Phase::CallWindow { pending, .. } | Phase::ChankanWindow { pending, .. } = g.phase {
        let others = pending & !(1 << keep);
        if others == 0 {
            break;
        }
        act(g, others.trailing_zeros() as u8, Action::Pass);
    }
}

/// Passes every open window, so play moves on.
fn all_pass(g: &mut GameState) {
    others_pass(g, 4);
}

fn win_of(events: &[Event]) -> Vec<(u8, u8, u8, u32)> {
    events
        .iter()
        .filter_map(|e| match *e {
            Event::Win {
                seat,
                han,
                fu,
                points,
                ..
            } => Some((seat, han, fu, points)),
            _ => None,
        })
        .collect()
}

#[test]
fn tenhou_is_a_yakuman_tsumo() {
    let mut g = game(["123m456p789s5566z", "", "", ""], "6z");
    assert!(g.legal_actions(0).contains(&Action::Tsumo));
    act(&mut g, 0, Action::Tsumo);
    assert_eq!(g.scores, [25_000 + 48_000, 9_000, 9_000, 9_000]);
    // The dealer won, so the dealer repeats.
    assert_eq!((g.round.index, g.round.honba), (0, 1));
}

#[test]
fn ron_with_tanyao_moves_the_dealer() {
    let mut g = game(["1119m1119p1119s6p", "234m345p345s88s66p", "", ""], "7z");
    act(&mut g, 0, Action::Discard(t("6p")));
    assert_eq!(
        g.legal_actions(1).as_slice(),
        &[
            Action::Ron,
            Action::Pon([t("6p"), t("6p")]),
            Action::Chi([t("4p"), t("5p")]),
            Action::Pass
        ]
    );
    others_pass(&mut g, 1);
    let ev = act(&mut g, 1, Action::Ron);
    // Tanyao; 20 + 10 closed ron + 2 for the ron-completed (open) 666p = 40 fu.
    assert_eq!(win_of(&ev), vec![(1, 1, 40, 1300)]);
    assert_eq!(g.scores, [23_700, 26_300, 25_000, 25_000]);
    assert_eq!(g.phase, Phase::RoundEnd);
    assert_eq!((g.round.index, g.round.honba), (1, 0));
}

#[test]
fn discard_furiten_blocks_ron() {
    // Seat 1 waits on 2s/5s, draws 5s and throws it away: furiten on both.
    let mut g = game(["", "234m567p34s888p22s", "", ""], "7z5s2s");
    act(&mut g, 0, Action::Discard(t("7z")));
    all_pass(&mut g);
    assert!(g.legal_actions(1).contains(&Action::Tsumo));
    act(&mut g, 1, Action::Discard(t("5s")));
    all_pass(&mut g);
    act(&mut g, 2, Action::Discard(t("2s")));
    let legal = g.legal_actions(1);
    assert!(
        !legal.contains(&Action::Ron),
        "furiten: {:?}",
        legal.as_slice()
    );
}

#[test]
fn double_ron_gives_honba_and_sticks_to_the_first_winner() {
    let mut g = game(
        [
            "1119m1119p1119s6p",
            "234m345p345s88s66p",
            "456m78p44p222s345s",
            "",
        ],
        "7z",
    );
    g.round.honba = 2;
    g.riichi_sticks = 1;
    g.scores[3] -= 1000;
    act(&mut g, 0, Action::Discard(t("6p")));
    let mut ev = act(&mut g, 2, Action::Ron);
    if g.waiting_on() & 0b1000 != 0 {
        act(&mut g, 3, Action::Pass);
    }
    ev.extend(act(&mut g, 1, Action::Ron));
    let wins = win_of(&ev);
    assert_eq!(wins, vec![(1, 1, 40, 1300), (2, 1, 40, 1300)]);
    // Seat 1: 1300 + 600 honba + 1000 stick. Seat 2: 1300.
    assert_eq!(g.scores, [21_800, 27_900, 26_300, 24_000]);
    assert_eq!(g.riichi_sticks, 0);
    assert_eq!((g.round.index, g.round.honba), (1, 0));
}

#[test]
fn chankan_robs_a_kakan() {
    // Dealer throws 6p: seat 1 pons it, seat 2 (kanchan on 6p) passes.
    // Later seat 1 draws the last 6p and adds it; seat 2 robs it.
    let mut g = game(
        [
            "1119m1119p1119s6p",
            "66p88m99m13s19p5z6z7z",
            "57p234m345s678s22m",
            "",
        ],
        "7z8s8s8s6p",
    );
    act(&mut g, 0, Action::Discard(t("6p")));
    assert!(g.legal_actions(2).contains(&Action::Ron));
    act(&mut g, 2, Action::Pass);
    others_pass(&mut g, 1);
    act(&mut g, 1, Action::Pon([t("6p"), t("6p")]));
    act(&mut g, 1, Action::Discard(t("7z")));
    all_pass(&mut g);
    act(&mut g, 2, Action::Discard(t("8s")));
    all_pass(&mut g);
    act(&mut g, 3, Action::Discard(t("8s")));
    all_pass(&mut g);
    act(&mut g, 0, Action::Discard(t("8s")));
    all_pass(&mut g);
    assert!(g.legal_actions(1).contains(&Action::Kakan(t("6p"))));
    act(&mut g, 1, Action::Kakan(t("6p")));
    assert_eq!(g.legal_actions(2).as_slice(), &[Action::Ron, Action::Pass]);
    others_pass(&mut g, 2);
    let ev = act(&mut g, 2, Action::Ron);
    // Chankan + tanyao, 20 + 10 + 2 (kanchan) = 32 -> 40 fu.
    assert_eq!(win_of(&ev), vec![(2, 2, 40, 2600)]);
    assert_eq!(g.scores[1], 25_000 - 2600);
}

#[test]
fn kuikae_forbids_the_called_tile_and_its_suji() {
    let mut g = game(["1119m1119p1119s4m", "4567m22p88p13s567z", "", ""], "7z");
    act(&mut g, 0, Action::Discard(t("4m")));
    others_pass(&mut g, 1);
    act(&mut g, 1, Action::Chi([t("5m"), t("6m")]));
    let legal = g.legal_actions(1);
    assert!(!legal.contains(&Action::Discard(t("4m"))));
    assert!(!legal.contains(&Action::Discard(t("7m"))));
    assert!(legal.contains(&Action::Discard(t("2p"))));
}

#[test]
fn ronned_riichi_discard_cancels_the_riichi() {
    let mut g = game(["", "234m567p34s888p22s", "234m456s789s11p66z", ""], "7z6z");
    act(&mut g, 0, Action::Discard(t("7z")));
    all_pass(&mut g);
    act(&mut g, 1, Action::Riichi(t("6z")));
    others_pass(&mut g, 2);
    let ev = act(&mut g, 2, Action::Ron);
    // Hatsu; 20 + 10 + 4 (ron-completed honor triplet) = 34 -> 40 fu.
    assert_eq!(win_of(&ev), vec![(2, 1, 40, 1300)]);
    assert_eq!(g.scores, [25_000, 23_700, 26_300, 25_000]);
    assert_eq!(g.riichi_sticks, 0);
}

#[test]
fn double_riichi_ippatsu_ron() {
    let mut g = game(["", "234m567p34s888p22s", "", ""], "7z9m5s");
    act(&mut g, 0, Action::Discard(t("7z")));
    all_pass(&mut g);
    // Riichi on the first discard with no calls yet is double riichi.
    act(&mut g, 1, Action::Riichi(t("9m")));
    all_pass(&mut g);
    assert_eq!((g.scores[1], g.riichi_sticks), (24_000, 1));
    // After riichi only the drawn tile may be discarded.
    act(&mut g, 2, Action::Discard(t("5s")));
    others_pass(&mut g, 1);
    let ev = act(&mut g, 1, Action::Ron);
    // Double riichi, ippatsu, tanyao; 20 + 10 + 4 (closed 888p) = 34 -> 40 fu.
    assert_eq!(win_of(&ev), vec![(1, 4, 40, 8000)]);
    assert_eq!(g.scores[1], 24_000 + 8000 + 1000);
    assert_eq!(g.scores[2], 25_000 - 8000);
}

#[test]
fn suufon_renda_aborts() {
    let mut g = game(["", "", "", ""], "4z4z4z4z");
    let mut ev = Vec::new();
    for seat in 0..4 {
        ev.extend(act(&mut g, seat, Action::Discard(t("4z"))));
        all_pass(&mut g);
    }
    assert!(ev.contains(&Event::AbortiveDraw {
        kind: AbortKind::SuufonRenda
    }));
    assert_eq!(g.phase, Phase::RoundEnd);
    assert_eq!((g.round.index, g.round.honba), (0, 1));
}

#[test]
fn kyuushu_kyuuhai_is_offered_and_aborts() {
    let mut g = game(["19m19p19s4567z5m2p3p", "", "", ""], "8p");
    assert!(g.legal_actions(0).contains(&Action::Kyuushu));
    let ev = act(&mut g, 0, Action::Kyuushu);
    assert!(ev.contains(&Event::AbortiveDraw {
        kind: AbortKind::KyuushuKyuuhai
    }));
    assert_eq!(g.phase, Phase::RoundEnd);
}

#[test]
fn daisangen_liability_on_tsumo() {
    let mut g = game(
        ["1119m1119p1119s5z", "55z66z77z123m9m1p2p3p", "6z", "7z"],
        "1s4z4z4z2s3s4s9m",
    );
    act(&mut g, 0, Action::Discard(t("5z")));
    others_pass(&mut g, 1);
    act(&mut g, 1, Action::Pon([t("5z"), t("5z")]));
    act(&mut g, 1, Action::Discard(t("1p")));
    all_pass(&mut g);
    act(&mut g, 2, Action::Discard(t("6z")));
    others_pass(&mut g, 1);
    act(&mut g, 1, Action::Pon([t("6z"), t("6z")]));
    act(&mut g, 1, Action::Discard(t("2p")));
    all_pass(&mut g);
    act(&mut g, 2, Action::Discard(t("4z")));
    all_pass(&mut g);
    act(&mut g, 3, Action::Discard(t("7z")));
    others_pass(&mut g, 1);
    act(&mut g, 1, Action::Pon([t("7z"), t("7z")]));
    act(&mut g, 1, Action::Discard(t("3p")));
    all_pass(&mut g);
    for seat in [2, 3, 0] {
        let drawn = match seat {
            2 => "2s",
            3 => "3s",
            _ => "4s",
        };
        act(&mut g, seat, Action::Discard(t(drawn)));
        all_pass(&mut g);
    }
    assert!(g.legal_actions(1).contains(&Action::Tsumo));
    let ev = act(&mut g, 1, Action::Tsumo);
    let deltas = ev.iter().find_map(|e| match e {
        Event::RoundEnded { deltas } => Some(*deltas),
        _ => None,
    });
    // Seat 3 fed the third dragon set and pays the whole yakuman.
    assert_eq!(deltas, Some([0, 32_000, 0, -32_000]));
    assert!(g.is_over(), "seat 3 went below zero");
}

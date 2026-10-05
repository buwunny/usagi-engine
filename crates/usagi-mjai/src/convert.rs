//! Engine events to Mjai events.

use usagi_core::hand::MeldKind;
use usagi_core::{Suit, Tile};
use usagi_engine::game::Event as EngineEvent;
use usagi_engine::state::NO_SEAT;
use usagi_engine::{Action, GameState, Rules};

use crate::event::{Event, Pai};

fn is_five(kind: u8) -> bool {
    kind < 27 && kind % 9 == 4
}

fn red_of(kind: u8) -> Tile {
    Tile::red_five(match kind / 9 {
        0 => Suit::Man,
        1 => Suit::Pin,
        _ => Suit::Sou,
    })
}

/// `n` tiles of `kind`, one of them red when `red`.
fn copies(kind: u8, n: usize, red: bool) -> Vec<Pai> {
    let mut v: Vec<Pai> = (0..n).map(|_| Tile::from_kind(kind).into()).collect();
    if red {
        v[0] = red_of(kind).into();
    }
    v
}

/// The `start_kyoku` for the hand just dealt in `g` (the dealer's first
/// draw is a separate `tsumo`).
pub fn start_kyoku<R: Rules>(g: &GameState<R>) -> Event {
    let dealer = g.dealer();
    let tehais = [0u8, 1, 2, 3].map(|s| {
        let mut tiles = g.concealed_tiles(s);
        if s == dealer {
            let drawn = g.players[s as usize].drawn;
            let i = tiles
                .iter()
                .position(|t| t.code() == drawn)
                .expect("dealer holds the drawn tile");
            tiles.remove(i);
        }
        tiles.into_iter().map(Pai::from).collect()
    });
    Event::StartKyoku {
        bakaze: Tile::from_kind(g.round_wind()).into(),
        dora_marker: g.dora_indicators()[0].into(),
        kyoku: g.round.index % 4 + 1,
        honba: g.round.honba,
        kyotaku: g.riichi_sticks,
        oya: dealer,
        scores: g.scores,
        tehais,
    }
}

/// The Mjai events for one engine event, with full information. `g` is the
/// state right after the step that produced `e`.
pub fn convert<R: Rules>(g: &GameState<R>, e: &EngineEvent, out: &mut Vec<Event>) {
    match *e {
        EngineEvent::RoundStarted { .. } => out.push(start_kyoku(g)),
        EngineEvent::Draw { seat, tile, .. } => out.push(Event::Tsumo {
            actor: seat,
            pai: tile.into(),
        }),
        EngineEvent::Discard {
            seat,
            tile,
            tsumogiri,
            riichi,
        } => {
            if riichi {
                out.push(Event::Reach { actor: seat });
            }
            out.push(Event::Dahai {
                actor: seat,
                pai: tile.into(),
                tsumogiri,
            });
        }
        EngineEvent::RiichiAccepted { seat } => out.push(Event::ReachAccepted { actor: seat }),
        EngineEvent::Call {
            seat,
            from,
            tile,
            action,
        } => out.push(match action {
            Action::Chi([a, b]) => Event::Chi {
                actor: seat,
                target: from,
                pai: tile.into(),
                consumed: vec![a.into(), b.into()],
            },
            Action::Pon([a, b]) => Event::Pon {
                actor: seat,
                target: from,
                pai: tile.into(),
                consumed: vec![a.into(), b.into()],
            },
            _ => {
                let k = tile.kind();
                Event::Daiminkan {
                    actor: seat,
                    target: from,
                    pai: tile.into(),
                    consumed: copies(k, 3, R::RED_FIVES > 0 && is_five(k) && !tile.is_red()),
                }
            }
        }),
        EngineEvent::Kan { seat, action } => out.push(match action {
            Action::Ankan(k) => Event::Ankan {
                actor: seat,
                consumed: copies(k, 4, R::RED_FIVES > 0 && is_five(k)),
            },
            Action::Kakan(t) => {
                let k = t.kind();
                let pon_red = g.players[seat as usize]
                    .melds()
                    .iter()
                    .find(|m| m.first() == k && matches!(m.kind(), MeldKind::Pon | MeldKind::Kakan))
                    .is_some_and(|m| m.has_red());
                Event::Kakan {
                    actor: seat,
                    pai: t.into(),
                    consumed: copies(k, 3, pon_red && !t.is_red()),
                }
            }
            other => unreachable!("Kan event with {other:?}"),
        }),
        EngineEvent::NewDora { indicator } => out.push(Event::Dora {
            dora_marker: indicator.into(),
        }),
        EngineEvent::Win {
            seat, from, tile, ..
        } => out.push(Event::Hora {
            actor: seat,
            target: if from == NO_SEAT { seat } else { from },
            pai: Some(tile.into()),
            deltas: None,
        }),
        EngineEvent::ExhaustiveDraw { .. } | EngineEvent::AbortiveDraw { .. } => {
            out.push(Event::Ryukyoku {
                actor: None,
                deltas: None,
            })
        }
        EngineEvent::RoundEnded { .. } => out.push(Event::EndKyoku),
        EngineEvent::GameEnded { .. } => out.push(Event::EndGame),
    }
}

/// `e` as `seat` may see it: other seats' draws and starting hands become
/// `?`.
pub fn mask(seat: u8, e: &Event) -> Event {
    match e {
        Event::Tsumo { actor, .. } if *actor != seat => Event::Tsumo {
            actor: *actor,
            pai: Pai::UNKNOWN,
        },
        Event::StartKyoku {
            bakaze,
            dora_marker,
            kyoku,
            honba,
            kyotaku,
            oya,
            scores,
            tehais,
        } => Event::StartKyoku {
            bakaze: *bakaze,
            dora_marker: *dora_marker,
            kyoku: *kyoku,
            honba: *honba,
            kyotaku: *kyotaku,
            oya: *oya,
            scores: *scores,
            tehais: std::array::from_fn(|s| {
                if s == seat as usize {
                    tehais[s].clone()
                } else {
                    vec![Pai::UNKNOWN; tehais[s].len()]
                }
            }),
        },
        other => other.clone(),
    }
}

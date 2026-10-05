//! Points and payments. See `docs/reference/scoring.md`.
//!
//! Score flow: han + fu -> *base points* -> what each player pays.

use crate::counts::Counts;
use crate::hand::Meld;
use crate::tile::Tile;
use crate::yaku::{WinContext, Yaku};

/// Base points from han and fu, with the limit hands applied.
///
/// - Below mangan: `fu * 2^(2 + han)`, capped at 2000.
/// - 5 han: mangan, 2000.
/// - 6-7 han: haneman, 3000.
/// - 8-10 han: baiman, 4000.
/// - 11-12 han: sanbaiman, 6000.
/// - 13+ han (kazoe yakuman): 8000.
///
/// Tenhou has **no kiriage mangan**: 4 han 30 fu stays 1920 base
/// (7700 points from a non-dealer), it is not rounded up to mangan.
pub fn base_points(han: u8, fu: u8) -> u32 {
    match han {
        13.. => 8000,
        11..=12 => 6000,
        8..=10 => 4000,
        6..=7 => 3000,
        5 => 2000,
        _ => (fu as u32 * (1u32 << (2 + han))).min(2000),
    }
}

/// Base points for `count` yakuman (stacked yakuman are multiples of 8000).
pub fn yakuman_base(count: u8) -> u32 {
    8000 * count as u32
}

/// Rounds up to the next multiple of 100.
pub fn round_up_100(points: u32) -> u32 {
    points.div_ceil(100) * 100
}

/// Who pays what for a win, before honba and riichi sticks.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Payment {
    /// The discarder pays everything.
    Ron(u32),
    /// Dealer won by tsumo: each of the three others pays this.
    DealerTsumo { each: u32 },
    /// Non-dealer won by tsumo: the dealer pays `dealer`, the other two
    /// non-dealers pay `non_dealer` each.
    NonDealerTsumo { dealer: u32, non_dealer: u32 },
}

impl Payment {
    /// Total points the winner receives (honba and sticks not included).
    pub fn total(self) -> u32 {
        match self {
            Payment::Ron(p) => p,
            Payment::DealerTsumo { each } => each * 3,
            Payment::NonDealerTsumo { dealer, non_dealer } => dealer + 2 * non_dealer,
        }
    }
}

/// Turns base points into a payment.
///
/// - Ron: non-dealer winner gets base x 4, dealer winner base x 6.
/// - Tsumo by dealer: everyone pays base x 2.
/// - Tsumo by non-dealer: dealer pays base x 2, others base x 1.
///
/// Each individual payment is rounded up to 100 separately.
pub fn payment(base: u32, dealer: bool, tsumo: bool) -> Payment {
    match (dealer, tsumo) {
        (false, false) => Payment::Ron(round_up_100(base * 4)),
        (true, false) => Payment::Ron(round_up_100(base * 6)),
        (true, true) => Payment::DealerTsumo {
            each: round_up_100(base * 2),
        },
        (false, true) => Payment::NonDealerTsumo {
            dealer: round_up_100(base * 2),
            non_dealer: round_up_100(base),
        },
    }
}

/// The score change for each of the four seats after a win, honba and
/// riichi sticks included.
///
/// - `winner`, `dealer`: seat indices 0..=3.
/// - `discarder`: `Some(seat)` for ron, `None` for tsumo.
/// - Honba: +300 per honba on ron (all from the discarder), +100 per honba
///   from each payer on tsumo.
/// - Riichi sticks: the winner collects 1000 per stick on the table. That
///   money was already taken from the riichi declarers when they declared,
///   so here it is only added to the winner.
pub fn score_deltas(
    payment: Payment,
    winner: usize,
    discarder: Option<usize>,
    dealer: usize,
    honba: u8,
    riichi_sticks: u8,
) -> [i32; 4] {
    let mut d = [0i32; 4];
    let honba = honba as i32;
    match (payment, discarder) {
        (Payment::Ron(p), Some(from)) => {
            let p = p as i32 + 300 * honba;
            d[from] -= p;
            d[winner] += p;
        }
        (Payment::Ron(_), None) => panic!("ron needs a discarder"),
        (Payment::DealerTsumo { each }, _) => {
            for seat in 0..4 {
                if seat != winner {
                    let p = each as i32 + 100 * honba;
                    d[seat] -= p;
                    d[winner] += p;
                }
            }
        }
        (
            Payment::NonDealerTsumo {
                dealer: dp,
                non_dealer,
            },
            _,
        ) => {
            for seat in 0..4 {
                if seat != winner {
                    let base = if seat == dealer { dp } else { non_dealer };
                    let p = base as i32 + 100 * honba;
                    d[seat] -= p;
                    d[winner] += p;
                }
            }
        }
    }
    d[winner] += 1000 * riichi_sticks as i32;
    d
}

/// The full result of scoring a win.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WinResult {
    /// The yaku, sorted (by the enum's order). Dora are not in here.
    pub yaku: Vec<Yaku>,
    /// Han from yaku + dora + aka + ura (0 for yakuman hands).
    pub han: u8,
    pub fu: u8,
    pub dora: u8,
    pub aka: u8,
    pub ura: u8,
    /// Number of yakuman (0 for normal hands; kazoe yakuman is still 0
    /// here, it's a normal hand that hit 13 han).
    pub yakuman: u8,
    pub base: u32,
    pub payment: Payment,
}

/// Scores a winning hand. Returns `None` if the tiles aren't a winning
/// shape or the hand has no yaku.
///
/// - `concealed`: the concealed tiles **including** the winning tile.
/// - `melds`: called melds and closed kans.
///
/// When several readings exist, the one with the highest payment wins;
/// ties go to more han, then more fu.
pub fn score(concealed: &[Tile], melds: &[Meld], ctx: &WinContext) -> Option<WinResult> {
    use crate::hand::{MeldKind, is_chiitoitsu, is_kokushi, readings};
    use crate::yaku::{Riichi, chiitoitsu_yaku, situational_yaku, standard_yaku};
    let counts = Counts::from_tiles(concealed);
    let closed = melds.iter().all(|m| !m.kind.is_open());

    // All tiles, melds included, for dora.
    let mut all = counts;
    for m in melds {
        match m.kind {
            MeldKind::Chi => {
                for k in m.first..m.first + 3 {
                    all.0[k as usize] += 1;
                }
            }
            MeldKind::Pon => all.0[m.first as usize] += 3,
            _ => all.0[m.first as usize] += 4,
        }
    }
    let dora = count_dora(&all, &ctx.dora_indicators);
    let aka = concealed.iter().filter(|t| t.is_red()).count() as u8
        + melds.iter().map(|m| m.reds).sum::<u8>();
    let ura = if ctx.riichi != Riichi::None {
        count_dora(&all, &ctx.ura_indicators)
    } else {
        0
    };

    // (yaku, fu) candidates.
    let mut forms: Vec<(Vec<Yaku>, u8)> = Vec::new();
    let win_kind = ctx.win_tile.kind();
    for r in readings(&counts, melds, win_kind, !ctx.tsumo) {
        let y = standard_yaku(&r, ctx);
        let f = crate::fu::fu(&r, ctx);
        forms.push((y, f));
    }
    if melds.is_empty() && is_chiitoitsu(&counts) {
        forms.push((chiitoitsu_yaku(&counts, ctx), crate::fu::CHIITOITSU_FU));
    }
    if melds.is_empty() && is_kokushi(&counts) {
        let mut y: Vec<Yaku> = situational_yaku(ctx, true)
            .into_iter()
            .filter(|y| y.is_yakuman())
            .collect();
        y.push(Yaku::KokushiMusou);
        forms.push((y, 0));
    }

    let mut best: Option<WinResult> = None;
    for (mut yaku, f) in forms {
        if yaku.is_empty() {
            continue;
        }
        yaku.sort();
        let yakuman = yaku.iter().filter(|y| y.is_yakuman()).count() as u8;
        let result = if yakuman > 0 {
            let base = yakuman_base(yakuman);
            WinResult {
                yaku,
                han: 0,
                fu: 0,
                dora: 0,
                aka: 0,
                ura: 0,
                yakuman,
                base,
                payment: payment(base, ctx.is_dealer(), ctx.tsumo),
            }
        } else {
            let han = yaku.iter().map(|y| y.han(closed)).sum::<u8>() + dora + aka + ura;
            let base = base_points(han, f);
            WinResult {
                yaku,
                han,
                fu: f,
                dora,
                aka,
                ura,
                yakuman: 0,
                base,
                payment: payment(base, ctx.is_dealer(), ctx.tsumo),
            }
        };
        let better = match &best {
            None => true,
            Some(b) => {
                (result.payment.total(), result.han, result.fu) > (b.payment.total(), b.han, b.fu)
            }
        };
        if better {
            best = Some(result);
        }
    }
    best
}

/// Dora count for a hand: for each indicator, how many tiles of the
/// indicated dora kind the hand holds (melds included).
pub fn count_dora(all_tiles: &Counts, indicators: &[Tile]) -> u8 {
    indicators
        .iter()
        .map(|t| all_tiles.get(crate::tile::dora_from_indicator(t.kind())))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    // cargo test -p usagi-core score::

    #[test]
    fn base_point_table() {
        assert_eq!(base_points(1, 30), 240);
        assert_eq!(base_points(2, 25), 400);
        assert_eq!(base_points(3, 20), 640);
        assert_eq!(base_points(4, 30), 1920); // no kiriage on Tenhou
        assert_eq!(base_points(3, 70), 2000); // capped: mangan
        assert_eq!(base_points(4, 40), 2000);
        assert_eq!(base_points(5, 30), 2000);
        assert_eq!(base_points(6, 30), 3000);
        assert_eq!(base_points(7, 30), 3000);
        assert_eq!(base_points(8, 30), 4000);
        assert_eq!(base_points(10, 30), 4000);
        assert_eq!(base_points(11, 30), 6000);
        assert_eq!(base_points(12, 30), 6000);
        assert_eq!(base_points(13, 30), 8000);
        assert_eq!(base_points(20, 30), 8000);
        assert_eq!(yakuman_base(1), 8000);
        assert_eq!(yakuman_base(2), 16000);
    }

    #[test]
    fn rounding() {
        assert_eq!(round_up_100(960), 1000);
        assert_eq!(round_up_100(1000), 1000);
        assert_eq!(round_up_100(1001), 1100);
        assert_eq!(round_up_100(0), 0);
    }

    #[test]
    fn payments() {
        // 1 han 30 fu: 1000 ron from a non-dealer, 1500 from the dealer.
        assert_eq!(payment(240, false, false), Payment::Ron(1000));
        assert_eq!(payment(240, true, false), Payment::Ron(1500));
        // 3 han 20 fu (riichi pinfu tsumo): 700 / 1300.
        assert_eq!(
            payment(640, false, true),
            Payment::NonDealerTsumo {
                dealer: 1300,
                non_dealer: 700
            }
        );
        assert_eq!(
            payment(640, true, true),
            Payment::DealerTsumo { each: 1300 }
        );
        // Mangan.
        assert_eq!(payment(2000, false, false), Payment::Ron(8000));
        assert_eq!(payment(2000, true, false), Payment::Ron(12000));
        assert_eq!(
            payment(2000, false, true),
            Payment::NonDealerTsumo {
                dealer: 4000,
                non_dealer: 2000
            }
        );
        assert_eq!(payment(2000, false, true).total(), 8000);
        assert_eq!(payment(640, false, true).total(), 2700); // rounding adds 100
    }

    #[test]
    fn deltas_ron_with_honba_and_sticks() {
        // Seat 2 rons seat 0 for 3900, 2 honba, 1 riichi stick on the table.
        let d = score_deltas(Payment::Ron(3900), 2, Some(0), 1, 2, 1);
        assert_eq!(d, [-4500, 0, 5500, 0]);
    }

    #[test]
    fn deltas_tsumo_with_honba() {
        // Seat 1 (dealer is seat 3) tsumos 700/1300 with 1 honba.
        let p = Payment::NonDealerTsumo {
            dealer: 1300,
            non_dealer: 700,
        };
        let d = score_deltas(p, 1, None, 3, 1, 0);
        assert_eq!(d, [-800, 3000, -800, -1400]);
        // Dealer tsumo 4000 all, no honba, 2 sticks.
        let d = score_deltas(Payment::DealerTsumo { each: 4000 }, 0, None, 0, 0, 2);
        assert_eq!(d, [14000, -4000, -4000, -4000]);
    }

    #[test]
    fn dora_counting() {
        use crate::parse::{counts_of, tiles_of};
        // Indicator 4m -> dora 5m. Hand has two 5m.
        assert_eq!(count_dora(&counts_of("55m123p"), &tiles_of("4m")), 2);
        // Indicator 9s -> dora 1s; two indicators of the same kind count twice.
        assert_eq!(count_dora(&counts_of("1s"), &tiles_of("9s9s")), 2);
        // North -> East.
        assert_eq!(count_dora(&counts_of("111z"), &tiles_of("4z")), 3);
    }
}

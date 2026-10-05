//! The M3 case table.
//!
//! Every case is a complete winning hand plus its situation, and the
//! result Tenhou would give. Run them with:
//!
//!     cargo test -p usagi-core --test scoring_cases
//!
//! The test prints *every* failing case (not just the first).
//!
//! The plan's target is 200+ cases. These 38 are a starting set, checked
//! against an independent scorer. Add a case for every scoring bug fixed.
//!
//! ## Notation
//!
//! - `hand`: the concealed tiles **including** the winning tile.
//! - `melds`: `"chi:345m"`, `"pon:777z"`, `"kan:1111z"` (open kan),
//!   `"kakan:5555p"` (added kan), `"ankan:9999s"` (closed kan).
//! - `win`: the winning tile; must also appear in `hand`.
//! - `dora` / `ura`: indicator tiles, not the dora themselves.
//! - Seat defaults to South (non-dealer), round to East.

use usagi_core::hand::{Meld, MeldKind};
use usagi_core::score::{Payment, WinResult, score};
use usagi_core::tile::{EAST, SOUTH, WEST};
use usagi_core::yaku::Yaku::{self, *};
use usagi_core::yaku::{Riichi, WinContext};
use usagi_core::{parse_tiles, tiles_of};

struct Case {
    name: &'static str,
    hand: &'static str,
    melds: &'static [&'static str],
    win: &'static str,
    tsumo: bool,
    riichi: Riichi,
    ippatsu: bool,
    rinshan: bool,
    chankan: bool,
    haitei: bool,
    houtei: bool,
    seat: u8,
    round: u8,
    dora: &'static str,
    ura: &'static str,
    /// `None` means the hand must not score (no yaku, or not a win).
    expect: Option<Expect>,
}

struct Expect {
    yaku: &'static [Yaku],
    han: u8,
    fu: u8,
    dora: u8,
    aka: u8,
    ura: u8,
    yakuman: u8,
    payment: Payment,
}

const BASE: Case = Case {
    name: "",
    hand: "",
    melds: &[],
    win: "",
    tsumo: false,
    riichi: Riichi::None,
    ippatsu: false,
    rinshan: false,
    chankan: false,
    haitei: false,
    houtei: false,
    seat: SOUTH,
    round: EAST,
    dora: "",
    ura: "",
    expect: None,
};

const NO_EXTRAS: Expect = Expect {
    yaku: &[],
    han: 0,
    fu: 0,
    dora: 0,
    aka: 0,
    ura: 0,
    yakuman: 0,
    payment: Payment::Ron(0),
};

#[rustfmt::skip]
const CASES: &[Case] = &[
    Case {
        name: "riichi_pinfu_tsumo",
        hand: "234m56799p345678s",
        win: "8s",
        tsumo: true,
        riichi: Riichi::Riichi,
        expect: Some(Expect {
            yaku: &[Riichi, MenzenTsumo, Pinfu],
            han: 3,
            fu: 20,
            payment: Payment::NonDealerTsumo { dealer: 1300, non_dealer: 700 },
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "riichi_pinfu_ron",
        hand: "234m56799p345678s",
        win: "8s",
        riichi: Riichi::Riichi,
        expect: Some(Expect {
            yaku: &[Riichi, Pinfu],
            han: 2,
            fu: 30,
            payment: Payment::Ron(2000),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "open_tanyao_ron_gets_30_fu",
        hand: "456p23467888s",
        melds: &["chi:234m"],
        win: "2s",
        expect: Some(Expect {
            yaku: &[Tanyao],
            han: 1,
            fu: 30,
            payment: Payment::Ron(1000),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "open_tanyao_tsumo",
        hand: "456p23467888s",
        melds: &["chi:234m"],
        win: "2s",
        tsumo: true,
        expect: Some(Expect {
            yaku: &[Tanyao],
            han: 1,
            fu: 30,
            payment: Payment::NonDealerTsumo { dealer: 500, non_dealer: 300 },
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "chun_pon_double_east_pair_tanki",
        hand: "123m456p789s11z",
        melds: &["pon:777z"],
        win: "1z",
        seat: EAST,
        expect: Some(Expect {
            yaku: &[Chun],
            han: 1,
            fu: 30,
            payment: Payment::Ron(1500),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "riichi_chiitoitsu",
        hand: "1199m2255p3377s44z",
        win: "4z",
        riichi: Riichi::Riichi,
        expect: Some(Expect {
            yaku: &[Riichi, Chiitoitsu],
            han: 3,
            fu: 25,
            payment: Payment::Ron(3200),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "chiitoitsu_tsumo",
        hand: "1199m2255p3377s44z",
        win: "4z",
        tsumo: true,
        expect: Some(Expect {
            yaku: &[MenzenTsumo, Chiitoitsu],
            han: 3,
            fu: 25,
            payment: Payment::NonDealerTsumo { dealer: 1600, non_dealer: 800 },
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "open_honitsu_haku_tsumo_closed_triplet",
        hand: "12334577999m",
        melds: &["pon:555z"],
        win: "9m",
        tsumo: true,
        expect: Some(Expect {
            yaku: &[Haku, Honitsu],
            han: 3,
            fu: 40,
            payment: Payment::NonDealerTsumo { dealer: 2600, non_dealer: 1300 },
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "toitoi_ron_shanpon_triplet_is_open",
        hand: "777m333s44455z",
        melds: &["pon:222p"],
        win: "4z",
        expect: Some(Expect {
            yaku: &[Toitoi],
            han: 2,
            fu: 40,
            payment: Payment::Ron(2600),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "toitoi_sanankou_tsumo",
        hand: "777m333s44455z",
        melds: &["pon:222p"],
        win: "4z",
        tsumo: true,
        expect: Some(Expect {
            yaku: &[Toitoi, Sanankou],
            han: 4,
            fu: 50,
            payment: Payment::NonDealerTsumo { dealer: 4000, non_dealer: 2000 },
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "riichi_with_closed_kan",
        hand: "234m56766p345s",
        melds: &["ankan:9999s"],
        win: "2m",
        riichi: Riichi::Riichi,
        expect: Some(Expect {
            yaku: &[Riichi],
            han: 1,
            fu: 70,
            payment: Payment::Ron(2300),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "pinfu_tsumo_ittsu",
        hand: "123456789m234p55s",
        win: "9m",
        tsumo: true,
        expect: Some(Expect {
            yaku: &[MenzenTsumo, Pinfu, Ittsu],
            han: 4,
            fu: 20,
            payment: Payment::NonDealerTsumo { dealer: 2600, non_dealer: 1300 },
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "ittsu_tsumo_tanki",
        hand: "123456789m234p55s",
        win: "5s",
        tsumo: true,
        expect: Some(Expect {
            yaku: &[MenzenTsumo, Ittsu],
            han: 3,
            fu: 30,
            payment: Payment::NonDealerTsumo { dealer: 2000, non_dealer: 1000 },
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "junchan_sanshoku_penchan",
        hand: "123m123p789p99s123s",
        win: "3p",
        expect: Some(Expect {
            yaku: &[SanshokuDoujun, Junchan],
            han: 5,
            fu: 40,
            payment: Payment::Ron(8000),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "open_chanta_east_penchan",
        hand: "789p111z789s99m",
        melds: &["chi:123m"],
        win: "7s",
        expect: Some(Expect {
            yaku: &[RoundWind, Chanta],
            han: 2,
            fu: 30,
            payment: Payment::Ron(2000),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "ryanpeikou_beats_chiitoitsu",
        hand: "223344m556677p88s",
        win: "7p",
        expect: Some(Expect {
            yaku: &[Pinfu, Tanyao, Ryanpeikou],
            han: 5,
            fu: 30,
            payment: Payment::Ron(8000),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "chinitsu_ittsu_baiman",
        hand: "11223345556789s",
        win: "9s",
        expect: Some(Expect {
            yaku: &[Pinfu, Iipeikou, Ittsu, Chinitsu],
            han: 10,
            fu: 30,
            payment: Payment::Ron(16000),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "daisangen",
        hand: "777z123m99p",
        melds: &["pon:555z", "pon:666z"],
        win: "7z",
        expect: Some(Expect {
            yaku: &[Daisangen],
            han: 0,
            fu: 0,
            yakuman: 1,
            payment: Payment::Ron(32000),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "kokushi_dealer_tsumo",
        hand: "19m19p19s12345677z",
        win: "7z",
        tsumo: true,
        seat: EAST,
        expect: Some(Expect {
            yaku: &[KokushiMusou],
            han: 0,
            fu: 0,
            yakuman: 1,
            payment: Payment::DealerTsumo { each: 16000 },
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "suuankou_tsumo",
        hand: "111m333p555s77799s",
        win: "7s",
        tsumo: true,
        expect: Some(Expect {
            yaku: &[Suuankou],
            han: 0,
            fu: 0,
            yakuman: 1,
            payment: Payment::NonDealerTsumo { dealer: 16000, non_dealer: 8000 },
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "suuankou_ron_is_only_sanankou_toitoi",
        hand: "111m333p555s77799s",
        win: "7s",
        expect: Some(Expect {
            yaku: &[Toitoi, Sanankou],
            han: 4,
            fu: 50,
            payment: Payment::Ron(8000),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "riichi_haitei",
        hand: "234m56799p345678s",
        win: "8s",
        tsumo: true,
        riichi: Riichi::Riichi,
        haitei: true,
        expect: Some(Expect {
            yaku: &[Riichi, MenzenTsumo, Pinfu, Haitei],
            han: 4,
            fu: 20,
            payment: Payment::NonDealerTsumo { dealer: 2600, non_dealer: 1300 },
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "dora_and_aka",
        hand: "234m56799p340678s",
        win: "8s",
        riichi: Riichi::Riichi,
        dora: "1m",
        expect: Some(Expect {
            yaku: &[Riichi, Pinfu],
            han: 4,
            fu: 30,
            dora: 1,
            aka: 1,
            payment: Payment::Ron(7700),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "ura_dora",
        hand: "234m56799p345678s",
        win: "8s",
        riichi: Riichi::Riichi,
        ura: "8p",
        expect: Some(Expect {
            yaku: &[Riichi, Pinfu],
            han: 4,
            fu: 30,
            ura: 2,
            payment: Payment::Ron(7700),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "riichi_yakuhai_pair_not_pinfu",
        hand: "234m567p345678s55z",
        win: "8s",
        riichi: Riichi::Riichi,
        expect: Some(Expect {
            yaku: &[Riichi],
            han: 1,
            fu: 40,
            payment: Payment::Ron(1300),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "kazoe_yakuman",
        hand: "11223345556789s",
        win: "9s",
        tsumo: true,
        riichi: Riichi::Riichi,
        dora: "4s",
        expect: Some(Expect {
            yaku: &[Riichi, MenzenTsumo, Pinfu, Iipeikou, Ittsu, Chinitsu],
            han: 15,
            fu: 20,
            dora: 3,
            payment: Payment::NonDealerTsumo { dealer: 16000, non_dealer: 8000 },
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "kiriage_off_7700",
        hand: "234m56788p345678s",
        win: "8s",
        riichi: Riichi::Riichi,
        dora: "1m",
        expect: Some(Expect {
            yaku: &[Riichi, Pinfu, Tanyao],
            han: 4,
            fu: 30,
            dora: 1,
            payment: Payment::Ron(7700),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "rinshan_kaihou",
        hand: "234m567p34588s",
        melds: &["ankan:1111z"],
        win: "5s",
        tsumo: true,
        rinshan: true,
        expect: Some(Expect {
            yaku: &[MenzenTsumo, RoundWind, RinshanKaihou],
            han: 3,
            fu: 60,
            payment: Payment::NonDealerTsumo { dealer: 3900, non_dealer: 2000 },
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "sanankou_beats_iipeikou",
        hand: "111222333m45677p",
        win: "6p",
        riichi: Riichi::Riichi,
        expect: Some(Expect {
            yaku: &[Riichi, Sanankou],
            han: 3,
            fu: 50,
            payment: Payment::Ron(6400),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "shousangen_honroutou_toitoi",
        hand: "111m555z66z",
        melds: &["pon:777z", "pon:999s"],
        win: "1m",
        tsumo: true,
        expect: Some(Expect {
            yaku: &[Haku, Chun, Toitoi, Honroutou, Shousangen],
            han: 8,
            fu: 50,
            payment: Payment::NonDealerTsumo { dealer: 8000, non_dealer: 4000 },
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "open_kan_tanyao_with_red_five",
        hand: "234m22p345678s",
        melds: &["kakan:0555p"],
        win: "2m",
        expect: Some(Expect {
            yaku: &[Tanyao],
            han: 2,
            fu: 30,
            aka: 1,
            payment: Payment::Ron(2000),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "houtei",
        hand: "234m56799p345678s",
        win: "8s",
        houtei: true,
        seat: WEST,
        round: SOUTH,
        expect: Some(Expect {
            yaku: &[Pinfu, Houtei],
            han: 2,
            fu: 30,
            payment: Payment::Ron(2000),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "chuuren_poutou",
        hand: "11112345678999m",
        win: "1m",
        expect: Some(Expect {
            yaku: &[ChuurenPoutou],
            han: 0,
            fu: 0,
            yakuman: 1,
            payment: Payment::Ron(32000),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "double_riichi_ippatsu",
        hand: "234m56799p345678s",
        win: "8s",
        riichi: Riichi::Double,
        ippatsu: true,
        expect: Some(Expect {
            yaku: &[Ippatsu, Pinfu, DoubleRiichi],
            han: 4,
            fu: 30,
            payment: Payment::Ron(7700),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "chankan",
        hand: "234m56799p345678s",
        win: "8s",
        riichi: Riichi::Riichi,
        chankan: true,
        expect: Some(Expect {
            yaku: &[Riichi, Pinfu, Chankan],
            han: 3,
            fu: 30,
            payment: Payment::Ron(3900),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "dealer_mangan_ron",
        hand: "123m123p789p99s123s",
        win: "3p",
        seat: EAST,
        expect: Some(Expect {
            yaku: &[SanshokuDoujun, Junchan],
            han: 5,
            fu: 40,
            payment: Payment::Ron(12000),
            ..NO_EXTRAS
        }),
        ..BASE
    },
    Case {
        name: "no_yaku",
        hand: "456p23467888s",
        melds: &["chi:123m"],
        win: "2s",
        expect: None,
        ..BASE
    },
    Case {
        name: "dora_is_not_a_yaku",
        hand: "456p23467888s",
        melds: &["chi:123m"],
        win: "2s",
        dora: "1s",
        expect: None,
        ..BASE
    },
];

fn parse_meld(text: &str) -> Meld {
    let (kind, tiles) = text
        .split_once(':')
        .expect("meld must look like kind:tiles");
    let kind = match kind {
        "chi" => MeldKind::Chi,
        "pon" => MeldKind::Pon,
        "kan" => MeldKind::Daiminkan,
        "kakan" => MeldKind::Kakan,
        "ankan" => MeldKind::Ankan,
        other => panic!("unknown meld kind {other:?}"),
    };
    Meld::from_tiles(kind, &tiles_of(tiles))
}

fn run(case: &Case) -> Option<WinResult> {
    let hand = tiles_of(case.hand);
    let melds: Vec<Meld> = case.melds.iter().map(|m| parse_meld(m)).collect();
    let win_tile = parse_tiles(case.win).unwrap()[0];
    assert!(
        hand.contains(&win_tile),
        "{}: the winning tile must be part of `hand`",
        case.name
    );
    let ctx = WinContext {
        win_tile,
        tsumo: case.tsumo,
        riichi: case.riichi,
        ippatsu: case.ippatsu,
        rinshan: case.rinshan,
        chankan: case.chankan,
        haitei: case.haitei,
        houtei: case.houtei,
        tenhou: false,
        chiihou: false,
        seat_wind: case.seat,
        round_wind: case.round,
        dora_indicators: tiles_of(case.dora),
        ura_indicators: tiles_of(case.ura),
    };
    score(&hand, &melds, &ctx)
}

/// Compares one case and returns a description of every mismatch.
fn check(case: &Case) -> Vec<String> {
    let got = std::panic::catch_unwind(|| run(case));
    let got = match got {
        Ok(g) => g,
        Err(_) => return vec!["panicked".to_string()],
    };
    let mut problems = Vec::new();
    match (&case.expect, got) {
        (None, None) => {}
        (None, Some(r)) => problems.push(format!("should not score, but got {r:?}")),
        (Some(_), None) => problems.push("should score, but got None".to_string()),
        (Some(e), Some(r)) => {
            let mut want_yaku = e.yaku.to_vec();
            want_yaku.sort();
            let mut got_yaku = r.yaku.clone();
            got_yaku.sort();
            let fields = [
                ("han", e.han as u32, r.han as u32),
                ("fu", e.fu as u32, r.fu as u32),
                ("dora", e.dora as u32, r.dora as u32),
                ("aka", e.aka as u32, r.aka as u32),
                ("ura", e.ura as u32, r.ura as u32),
                ("yakuman", e.yakuman as u32, r.yakuman as u32),
            ];
            if want_yaku != got_yaku {
                problems.push(format!("yaku: want {want_yaku:?}, got {got_yaku:?}"));
            }
            for (field, want, got) in fields {
                if want != got {
                    problems.push(format!("{field}: want {want}, got {got}"));
                }
            }
            if e.payment != r.payment {
                problems.push(format!(
                    "payment: want {:?}, got {:?}",
                    e.payment, r.payment
                ));
            }
        }
    }
    problems
}

#[test]
fn scoring_cases() {
    // Silence the default panic message while we catch panics, so
    // the output stays readable. It's restored at the end.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let mut failures = Vec::new();
    for case in CASES {
        let problems = check(case);
        if !problems.is_empty() {
            failures.push(format!(
                "  {} ({}):\n    {}",
                case.name,
                case.hand,
                problems.join("\n    ")
            ));
        }
    }

    std::panic::set_hook(hook);
    let passed = CASES.len() - failures.len();
    println!("{passed}/{} scoring cases pass", CASES.len());
    assert!(
        failures.is_empty(),
        "{} of {} cases failed:\n{}",
        failures.len(),
        CASES.len(),
        failures.join("\n")
    );
}

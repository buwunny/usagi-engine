//! Yaku: the scoring patterns. The full list is in `docs/reference/yaku.md`.
//!
//! A hand can only win if it has at least one yaku. Dora (bonus tiles) add
//! han but are not yaku, so a hand with dora and no yaku can't win.
//!
//! Each yaku is worth some *han*. A few are worth one han less when the
//! hand is open (*kuisagari*), and some can't be scored open at all.

use crate::counts::Counts;
use crate::hand::Reading;
use crate::tile::Tile;

/// Every yaku Tenhou scores (four-player rules).
///
/// Yakuman are listed last. Tenhou has no double yakuman for a single
/// pattern (a 13-sided kokushi is one yakuman), but different yakuman in
/// the same hand do stack: daisangen + tsuuiisou is a double.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Yaku {
    // 1 han
    Riichi,
    Ippatsu,
    MenzenTsumo,
    Pinfu,
    Iipeikou,
    Tanyao,
    Haku,
    Hatsu,
    Chun,
    /// Triplet of your own seat wind.
    SeatWind,
    /// Triplet of the round wind. With [`Yaku::SeatWind`] on the same
    /// triplet (East as dealer in the East round) the hand gets both.
    RoundWind,
    RinshanKaihou,
    Chankan,
    Haitei,
    Houtei,
    // 2 han
    DoubleRiichi,
    Chiitoitsu,
    SanshokuDoujun,
    Ittsu,
    Chanta,
    Toitoi,
    Sanankou,
    SanshokuDoukou,
    Sankantsu,
    Honroutou,
    Shousangen,
    // 3 han
    Honitsu,
    Junchan,
    Ryanpeikou,
    // 6 han
    Chinitsu,
    // Yakuman
    KokushiMusou,
    Suuankou,
    Daisangen,
    Shousuushi,
    Daisuushi,
    Tsuuiisou,
    Ryuuiisou,
    Chinroutou,
    ChuurenPoutou,
    Suukantsu,
    Tenhou,
    Chiihou,
}

impl Yaku {
    /// Han for this yaku. `closed` is whether the hand is closed.
    ///
    /// Returns 0 when the yaku isn't allowed in an open hand (pinfu,
    /// iipeikou, riichi...). Yakuman return 13.
    pub fn han(self, closed: bool) -> u8 {
        use Yaku::*;
        match self {
            Riichi | Ippatsu | MenzenTsumo | Pinfu | Iipeikou => closed as u8,
            Tanyao | Haku | Hatsu | Chun | SeatWind | RoundWind | RinshanKaihou | Chankan
            | Haitei | Houtei => 1,
            DoubleRiichi | Chiitoitsu => 2 * closed as u8,
            SanshokuDoujun | Ittsu | Chanta => {
                if closed {
                    2
                } else {
                    1
                }
            }
            Toitoi | Sanankou | SanshokuDoukou | Sankantsu | Honroutou | Shousangen => 2,
            Honitsu | Junchan => {
                if closed {
                    3
                } else {
                    2
                }
            }
            Ryanpeikou => 3 * closed as u8,
            Chinitsu => {
                if closed {
                    6
                } else {
                    5
                }
            }
            KokushiMusou | Suuankou | ChuurenPoutou | Tenhou | Chiihou => 13 * closed as u8,
            Daisangen | Shousuushi | Daisuushi | Tsuuiisou | Ryuuiisou | Chinroutou | Suukantsu => {
                13
            }
        }
    }

    pub fn is_yakuman(self) -> bool {
        self >= Yaku::KokushiMusou
    }
}

/// Riichi status of the winner.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Riichi {
    #[default]
    None,
    Riichi,
    /// Riichi declared on your very first discard, with no calls before it.
    Double,
}

/// Everything about a win that isn't the tiles themselves.
///
/// `Default` gives a plain ron by a non-dealer in the East round with no
/// riichi and no dora, so tests can write
/// `WinContext { tsumo: true, ..WinContext::default() }`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct WinContext {
    /// The winning tile (red or not).
    pub win_tile: Tile,
    /// Won by self-draw (tsumo) rather than on a discard (ron).
    pub tsumo: bool,
    pub riichi: Riichi,
    /// Won within one go-around of declaring riichi, with no call in between.
    pub ippatsu: bool,
    /// Won on the replacement tile drawn after a kan.
    pub rinshan: bool,
    /// Won by robbing a kakan (ron on the tile someone added to their pon).
    pub chankan: bool,
    /// Tsumo on the very last drawable tile.
    pub haitei: bool,
    /// Ron on the very last discard.
    pub houtei: bool,
    /// Dealer wins on their first draw.
    pub tenhou: bool,
    /// Non-dealer wins on their first draw, with no calls before it.
    pub chiihou: bool,
    /// The winner's seat wind as a kind (EAST..=NORTH). East = dealer.
    pub seat_wind: u8,
    /// The round wind as a kind (EAST or SOUTH in a hanchan, maybe WEST).
    pub round_wind: u8,
    /// Dora indicators as tiles (the dora is the next tile, see
    /// `tile::dora_from_indicator`).
    pub dora_indicators: Vec<Tile>,
    /// Ura dora indicators; only count when the winner is in riichi.
    pub ura_indicators: Vec<Tile>,
}

impl Default for WinContext {
    fn default() -> Self {
        WinContext {
            win_tile: Tile::from_kind(0),
            tsumo: false,
            riichi: Riichi::None,
            ippatsu: false,
            rinshan: false,
            chankan: false,
            haitei: false,
            houtei: false,
            tenhou: false,
            chiihou: false,
            seat_wind: crate::tile::SOUTH,
            round_wind: crate::tile::EAST,
            dora_indicators: Vec::new(),
            ura_indicators: Vec::new(),
        }
    }
}

impl WinContext {
    pub fn is_dealer(&self) -> bool {
        self.seat_wind == crate::tile::EAST
    }
}

/// The yaku that don't depend on hand shape at all: riichi, ippatsu,
/// menzen tsumo, rinshan, chankan, haitei, houtei, tenhou, chiihou.
///
/// `closed` matters for menzen tsumo (closed hands only).
pub fn situational_yaku(ctx: &WinContext, closed: bool) -> Vec<Yaku> {
    let mut y = Vec::new();
    match ctx.riichi {
        self::Riichi::None => {}
        self::Riichi::Riichi => y.push(Yaku::Riichi),
        self::Riichi::Double => y.push(Yaku::DoubleRiichi),
    }
    if ctx.ippatsu {
        y.push(Yaku::Ippatsu);
    }
    if ctx.tsumo && closed {
        y.push(Yaku::MenzenTsumo);
    }
    if ctx.rinshan {
        y.push(Yaku::RinshanKaihou);
    }
    if ctx.chankan {
        y.push(Yaku::Chankan);
    }
    if ctx.haitei {
        y.push(Yaku::Haitei);
    }
    if ctx.houtei {
        y.push(Yaku::Houtei);
    }
    if ctx.tenhou {
        y.push(Yaku::Tenhou);
    }
    if ctx.chiihou {
        y.push(Yaku::Chiihou);
    }
    y
}

/// All yaku for one reading of a standard hand, situational ones included.
///
/// If any yakuman is present, return only the yakuman.
pub fn standard_yaku(reading: &Reading, ctx: &WinContext) -> Vec<Yaku> {
    use crate::hand::Set;
    use crate::tile::{CHUN, EAST, HAKU, HATSU, NORTH};
    let t = |k: u8| Tile::from_kind(k);
    let closed = reading.closed;
    let mut y = situational_yaku(ctx, closed);

    // Flattened view of the hand.
    let mut seqs: Vec<u8> = Vec::new();
    let mut trips: Vec<(u8, bool, bool)> = Vec::new(); // (kind, open, kan)
    for s in &reading.sets {
        match *s {
            Set::Sequence { first, .. } => seqs.push(first),
            Set::Triplet { kind, open } => trips.push((kind, open, false)),
            Set::Kan { kind, open } => trips.push((kind, open, true)),
        }
    }
    let pair = reading.pair;
    let mut counts = Counts::new();
    for &f in &seqs {
        for k in f..f + 3 {
            counts.0[k as usize] += 1;
        }
    }
    for &(k, _, kan) in &trips {
        counts.0[k as usize] += if kan { 4 } else { 3 };
    }
    counts.0[pair as usize] += 2;
    let kinds: Vec<u8> = counts.kinds().collect();
    let has_trip = |k: u8| trips.iter().any(|&(x, _, _)| x == k);

    // --- yakuman ---
    let mut ym = Vec::new();
    if ctx.tenhou {
        ym.push(Yaku::Tenhou);
    }
    if ctx.chiihou {
        ym.push(Yaku::Chiihou);
    }
    let concealed_trips = trips.iter().filter(|&&(_, open, _)| !open).count();
    if concealed_trips == 4 {
        ym.push(Yaku::Suuankou);
    }
    let dragon_trips = (HAKU..=CHUN).filter(|&k| has_trip(k)).count();
    if dragon_trips == 3 {
        ym.push(Yaku::Daisangen);
    }
    let wind_trips = (EAST..=NORTH).filter(|&k| has_trip(k)).count();
    if wind_trips == 4 {
        ym.push(Yaku::Daisuushi);
    } else if wind_trips == 3 && (EAST..=NORTH).contains(&pair) {
        ym.push(Yaku::Shousuushi);
    }
    if kinds.iter().all(|&k| k >= 27) {
        ym.push(Yaku::Tsuuiisou);
    }
    const GREEN: [u8; 6] = [19, 20, 21, 23, 25, HATSU];
    if kinds.iter().all(|k| GREEN.contains(k)) {
        ym.push(Yaku::Ryuuiisou);
    }
    if kinds.iter().all(|&k| t(k).is_terminal()) {
        ym.push(Yaku::Chinroutou);
    }
    let kans = trips.iter().filter(|&&(_, _, kan)| kan).count();
    if kans == 4 {
        ym.push(Yaku::Suukantsu);
    }
    if closed && kans == 0 && kinds.iter().all(|&k| k < 27 && k / 9 == kinds[0] / 9) {
        let base = kinds[0] / 9 * 9;
        let need = [3, 1, 1, 1, 1, 1, 1, 1, 3];
        if (0..9).all(|i| counts.0[(base + i) as usize] >= need[i as usize]) {
            ym.push(Yaku::ChuurenPoutou);
        }
    }
    if !ym.is_empty() {
        ym.sort();
        return ym;
    }

    // --- normal yaku ---
    if is_pinfu(reading, ctx) {
        y.push(Yaku::Pinfu);
    }
    if closed {
        let mut sorted = seqs.clone();
        sorted.sort();
        let mut peiko = 0;
        let mut i = 0;
        while i + 1 < sorted.len() {
            if sorted[i] == sorted[i + 1] {
                peiko += 1;
                i += 2;
            } else {
                i += 1;
            }
        }
        if peiko == 2 {
            y.push(Yaku::Ryanpeikou);
        } else if peiko == 1 {
            y.push(Yaku::Iipeikou);
        }
    }
    if kinds.iter().all(|&k| t(k).is_simple()) {
        y.push(Yaku::Tanyao);
    }
    if has_trip(HAKU) {
        y.push(Yaku::Haku);
    }
    if has_trip(HATSU) {
        y.push(Yaku::Hatsu);
    }
    if has_trip(CHUN) {
        y.push(Yaku::Chun);
    }
    if has_trip(ctx.seat_wind) {
        y.push(Yaku::SeatWind);
    }
    if has_trip(ctx.round_wind) {
        y.push(Yaku::RoundWind);
    }
    if seqs
        .iter()
        .any(|&f| f < 9 && seqs.contains(&(f + 9)) && seqs.contains(&(f + 18)))
    {
        y.push(Yaku::SanshokuDoujun);
    }
    if [0u8, 9, 18]
        .iter()
        .any(|&b| seqs.contains(&b) && seqs.contains(&(b + 3)) && seqs.contains(&(b + 6)))
    {
        y.push(Yaku::Ittsu);
    }
    let yao = |k: u8| t(k).is_terminal_or_honor();
    let all_blocks_yao = seqs.iter().all(|&f| yao(f) || yao(f + 2))
        && trips.iter().all(|&(k, _, _)| yao(k))
        && yao(pair);
    let has_honor = kinds.iter().any(|&k| k >= 27);
    if all_blocks_yao && !seqs.is_empty() {
        if has_honor {
            y.push(Yaku::Chanta);
        } else {
            y.push(Yaku::Junchan);
        }
    }
    if trips.len() == 4 {
        y.push(Yaku::Toitoi);
    }
    if concealed_trips == 3 {
        y.push(Yaku::Sanankou);
    }
    if trips
        .iter()
        .any(|&(k, _, _)| k < 9 && has_trip(k + 9) && has_trip(k + 18))
    {
        y.push(Yaku::SanshokuDoukou);
    }
    if kans == 3 {
        y.push(Yaku::Sankantsu);
    }
    if kinds.iter().all(|&k| yao(k)) {
        y.push(Yaku::Honroutou);
    }
    if dragon_trips == 2 && (HAKU..=CHUN).contains(&pair) {
        y.push(Yaku::Shousangen);
    }
    push_flush(&kinds, &mut y);
    y.sort();
    y
}

/// All yaku for a seven-pairs hand (chiitoitsu itself included).
pub fn chiitoitsu_yaku(concealed: &Counts, ctx: &WinContext) -> Vec<Yaku> {
    let kinds: Vec<u8> = concealed.kinds().collect();
    let mut ym = Vec::new();
    if ctx.tenhou {
        ym.push(Yaku::Tenhou);
    }
    if ctx.chiihou {
        ym.push(Yaku::Chiihou);
    }
    if kinds.iter().all(|&k| k >= 27) {
        ym.push(Yaku::Tsuuiisou);
    }
    if !ym.is_empty() {
        ym.sort();
        return ym;
    }
    let mut y = situational_yaku(ctx, true);
    y.push(Yaku::Chiitoitsu);
    if kinds.iter().all(|&k| Tile::from_kind(k).is_simple()) {
        y.push(Yaku::Tanyao);
    }
    if kinds
        .iter()
        .all(|&k| Tile::from_kind(k).is_terminal_or_honor())
    {
        y.push(Yaku::Honroutou);
    }
    push_flush(&kinds, &mut y);
    y.sort();
    y
}

/// Honitsu / chinitsu from the kinds in the hand.
fn push_flush(kinds: &[u8], y: &mut Vec<Yaku>) {
    let suits: Vec<u8> = kinds.iter().filter(|&&k| k < 27).map(|&k| k / 9).collect();
    if suits.is_empty() || !suits.iter().all(|&s| s == suits[0]) {
        return;
    }
    if kinds.iter().any(|&k| k >= 27) {
        y.push(Yaku::Honitsu);
    } else {
        y.push(Yaku::Chinitsu);
    }
}

/// Pinfu needs the reading *and* the winds (the pair can't be a yakuhai).
/// It's public because fu calculation also needs to know.
pub fn is_pinfu(reading: &Reading, ctx: &WinContext) -> bool {
    use crate::hand::{Set, Wait};
    reading.closed
        && reading
            .sets
            .iter()
            .all(|s| matches!(s, Set::Sequence { .. }))
        && reading.pair < crate::tile::HAKU
        && reading.pair != ctx.seat_wind
        && reading.pair != ctx.round_wind
        && reading.wait == Wait::Ryanmen
}

#[cfg(test)]
mod tests {
    use super::*;

    // cargo test -p mochitsuki-core yaku::
    // Full hands are tested in tests/scoring_cases.rs.

    #[test]
    fn han_values() {
        assert_eq!(Yaku::Riichi.han(true), 1);
        assert_eq!(Yaku::Riichi.han(false), 0);
        assert_eq!(Yaku::Tanyao.han(false), 1); // kuitan: open tanyao is fine on Tenhou
        assert_eq!(Yaku::Pinfu.han(false), 0);
        assert_eq!(Yaku::Chiitoitsu.han(true), 2);
        assert_eq!(Yaku::Ittsu.han(true), 2);
        assert_eq!(Yaku::Ittsu.han(false), 1);
        assert_eq!(Yaku::Toitoi.han(false), 2);
        assert_eq!(Yaku::Honitsu.han(true), 3);
        assert_eq!(Yaku::Honitsu.han(false), 2);
        assert_eq!(Yaku::Ryanpeikou.han(false), 0);
        assert_eq!(Yaku::Chinitsu.han(true), 6);
        assert_eq!(Yaku::Chinitsu.han(false), 5);
        assert_eq!(Yaku::Daisangen.han(false), 13);
        assert!(Yaku::Daisangen.is_yakuman());
        assert!(!Yaku::Chinitsu.is_yakuman());
    }

    #[test]
    fn situational() {
        let ctx = WinContext {
            tsumo: true,
            riichi: Riichi::Riichi,
            ippatsu: true,
            ..WinContext::default()
        };
        let mut y = situational_yaku(&ctx, true);
        y.sort();
        assert_eq!(y, vec![Yaku::Riichi, Yaku::Ippatsu, Yaku::MenzenTsumo]);

        // Open hand: no menzen tsumo.
        let ctx = WinContext {
            tsumo: true,
            haitei: true,
            ..WinContext::default()
        };
        assert_eq!(situational_yaku(&ctx, false), vec![Yaku::Haitei]);

        // Double riichi replaces riichi.
        let ctx = WinContext {
            riichi: Riichi::Double,
            ..WinContext::default()
        };
        assert_eq!(situational_yaku(&ctx, true), vec![Yaku::DoubleRiichi]);
    }
}

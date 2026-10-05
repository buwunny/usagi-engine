//! Tenhou's mjlog format, as a typed event stream.
//!
//! Tiles in a log are *ids* 0..=135: kind `id / 4`, and the four copies of
//! a kind are `id % 4`. With red fives on, ids 16, 52 and 88 (the first 5m,
//! 5p and 5s) are the red ones. Seats are absolute (0 = the East seat of
//! East 1), points in the file are in hundreds and are converted to points
//! here.
//!
//! Element reference:
//!
//! | Element | Meaning |
//! | --- | --- |
//! | `GO type=` | Room flags (red fives, kuitan, hanchan, sanma...) |
//! | `INIT` | Hand start: round, honba, sticks, dice, dora indicator, scores, dealer, the four starting hands |
//! | `T`/`U`/`V`/`W` + id | Seat 0/1/2/3 draws |
//! | `D`/`E`/`F`/`G` + id | Seat 0/1/2/3 discards |
//! | `N who m` | A call; `m` packs the meld (see [`decode_meld`]) |
//! | `REACH who step` | Riichi declared (step 1) and accepted (step 2) |
//! | `DORA hai` | A new dora indicator flips |
//! | `AGARI` | A win, with yaku, fu, points and score changes |
//! | `RYUUKYOKU` | A draw, exhaustive or abortive |
//! | `BYE` / `UN` | Disconnects and reconnects (ignored) |

use std::fmt;

use mochitsuki_core::Tile;

use crate::xml::{self, Element};

/// A tile as the log writes it, 0..=135.
pub type TileId = u8;

/// Room flags from `<GO type=...>`.
pub mod room {
    pub const NO_RED: u32 = 0x02;
    pub const NO_KUITAN: u32 = 0x04;
    pub const HANCHAN: u32 = 0x08;
    pub const SANMA: u32 = 0x10;
}

/// The engine tile for log id `id`.
pub fn tile_of(id: TileId, red_fives: bool) -> Tile {
    if red_fives {
        match id {
            16 => return Tile::from_code(34).unwrap(),
            52 => return Tile::from_code(35).unwrap(),
            88 => return Tile::from_code(36).unwrap(),
            _ => {}
        }
    }
    Tile::from_kind(id / 4)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogError(pub String);

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LogError {}

impl From<xml::XmlError> for LogError {
    fn from(e: xml::XmlError) -> Self {
        LogError(format!("xml: {e}"))
    }
}

fn err<T>(msg: impl Into<String>) -> Result<T, LogError> {
    Err(LogError(msg.into()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    Chi,
    Pon,
    /// Added kan (a pon plus the fourth tile).
    Kakan,
    /// Open kan on a discard.
    Daiminkan,
    /// Closed kan.
    Ankan,
}

/// A decoded `N` element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub kind: CallKind,
    /// Every tile in the meld after the call, lowest id first.
    pub tiles: Vec<TileId>,
    /// The tile taken from another player (chi, pon, daiminkan), or the
    /// tile added (kakan). `None` for ankan.
    pub taken: Option<TileId>,
    /// Who it came from, relative to the caller: 1 right, 2 across,
    /// 3 left; 0 for ankan.
    pub from: u8,
}

impl Call {
    /// The tiles that came out of the caller's hand for this call.
    pub fn from_hand(&self) -> Vec<TileId> {
        match self.kind {
            CallKind::Kakan => self.taken.into_iter().collect(),
            _ => self
                .tiles
                .iter()
                .copied()
                .filter(|&t| Some(t) != self.taken)
                .collect(),
        }
    }
}

/// Unpacks the `m` attribute of an `N` element.
///
/// ```text
/// chi:   bit 2 set.  bits 10..: (base sequence index * 3 + called index);
///        base index t covers 21 sequences: kind = (t / 7) * 9 + t % 7.
///        bits 3-4, 5-6, 7-8: which copy (0..=3) of each of the 3 kinds.
/// pon:   bit 3 set.  bits 9..: (kind * 3 + called index among the three);
///        bits 5-6: the copy *not* in the pon.
/// kakan: bit 4 set.  same as pon; bits 5-6 is the added copy.
/// kan:   none of bits 2-5. bits 8..: the called (or first) tile id.
/// bits 0-1 (all kinds): who it came from, relative.
/// ```
pub fn decode_meld(m: u32) -> Result<Call, LogError> {
    let from = (m & 3) as u8;
    if m & 0x4 != 0 {
        let t0 = m >> 10;
        let called = (t0 % 3) as usize;
        let t = t0 / 3;
        if t >= 21 {
            return err(format!("bad chi meld {m}"));
        }
        let base = (t / 7) * 9 + t % 7;
        let tiles: Vec<TileId> = (0..3)
            .map(|i| ((base + i) * 4 + ((m >> (3 + 2 * i)) & 3)) as TileId)
            .collect();
        Ok(Call {
            kind: CallKind::Chi,
            taken: Some(tiles[called]),
            tiles,
            from,
        })
    } else if m & 0x18 != 0 {
        let t0 = m >> 9;
        let called = (t0 % 3) as usize;
        let kind = t0 / 3;
        if kind >= 34 {
            return err(format!("bad pon meld {m}"));
        }
        let unused = (m >> 5) & 3;
        let pon: Vec<TileId> = (0..4)
            .filter(|&i| i != unused)
            .map(|i| (kind * 4 + i) as TileId)
            .collect();
        if m & 0x8 != 0 {
            Ok(Call {
                kind: CallKind::Pon,
                taken: Some(pon[called]),
                tiles: pon,
                from,
            })
        } else {
            Ok(Call {
                kind: CallKind::Kakan,
                taken: Some((kind * 4 + unused) as TileId),
                tiles: (0..4).map(|i| (kind * 4 + i) as TileId).collect(),
                from,
            })
        }
    } else if m & 0x20 != 0 {
        err("nukidora (three-player) is not supported")
    } else {
        let hai0 = m >> 8;
        let kind = hai0 / 4;
        if kind >= 34 {
            return err(format!("bad kan meld {m}"));
        }
        let tiles = (0..4).map(|i| (kind * 4 + i) as TileId).collect();
        if from == 0 {
            Ok(Call {
                kind: CallKind::Ankan,
                tiles,
                taken: None,
                from,
            })
        } else {
            Ok(Call {
                kind: CallKind::Daiminkan,
                tiles,
                taken: Some(hai0 as TileId),
                from,
            })
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Init {
    /// 0..=3 East 1-4, 4..=7 South, 8..=11 West.
    pub round: u8,
    pub honba: u8,
    pub sticks: u8,
    pub dice: [u8; 2],
    pub dora_indicator: TileId,
    pub scores: [i32; 4],
    pub dealer: u8,
    pub hands: [Vec<TileId>; 4],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agari {
    pub who: u8,
    /// Equal to `who` for tsumo.
    pub from: u8,
    pub honba: u8,
    pub sticks: u8,
    /// The concealed tiles including the winning tile.
    pub hand: Vec<TileId>,
    pub melds: Vec<Call>,
    pub win_tile: TileId,
    pub fu: u32,
    /// The hand's value before honba and sticks.
    pub points: u32,
    /// 0 none, 1 mangan, 2 haneman, 3 baiman, 4 sanbaiman, 5 yakuman.
    pub limit: u8,
    /// `(yaku id, han)` pairs, dora counted as yaku 52 (dora), 53 (ura)
    /// and 54 (aka).
    pub yaku: Vec<(u8, u8)>,
    pub yakuman: Vec<u8>,
    pub dora_indicators: Vec<TileId>,
    pub ura_indicators: Vec<TileId>,
    /// Who is liable (pao), if anyone.
    pub pao: Option<u8>,
    /// Score changes in points.
    pub deltas: [i32; 4],
    /// Final scores in points, when this ended the game.
    pub owari: Option<[i32; 4]>,
}

impl Agari {
    /// Han including dora, or 13 per yakuman.
    pub fn han(&self) -> u32 {
        if self.yakuman.is_empty() {
            self.yaku.iter().map(|&(_, h)| h as u32).sum()
        } else {
            13 * self.yakuman.len() as u32
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawKind {
    /// Ran out of tiles.
    Exhaustive,
    /// Nine terminals and honors (`yao9`).
    Kyuushu,
    /// Four riichi (`reach4`).
    SuuchaRiichi,
    /// Triple ron (`ron3`).
    Sanchahou,
    /// Four kans by more than one player (`kan4`).
    Suukaikan,
    /// Four of the same wind discarded on the first go-around (`kaze4`).
    SuufonRenda,
    /// Exhaustive draw with nagashi mangan (`nm`).
    NagashiMangan,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ryuukyoku {
    pub kind: DrawKind,
    pub honba: u8,
    pub sticks: u8,
    pub deltas: [i32; 4],
    /// Seats that showed their hand (tenpai at an exhaustive draw).
    pub shown: [bool; 4],
    pub owari: Option<[i32; 4]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogEvent {
    Init(Init),
    Draw { seat: u8, tile: TileId },
    Discard { seat: u8, tile: TileId },
    Call { seat: u8, call: Call },
    Riichi { seat: u8, step: u8 },
    Dora { indicator: TileId },
    Agari(Box<Agari>),
    Ryuukyoku(Box<Ryuukyoku>),
}

/// One logged game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    /// `<GO type=...>` flags; see [`room`].
    pub room: u32,
    pub events: Vec<LogEvent>,
}

impl Game {
    pub fn red_fives(&self) -> bool {
        self.room & room::NO_RED == 0
    }

    pub fn hanchan(&self) -> bool {
        self.room & room::HANCHAN != 0
    }

    /// Four players, red fives and open tanyao: the rules mochitsuki plays.
    pub fn is_tenhou_ranked_rules(&self) -> bool {
        self.room & (room::NO_RED | room::NO_KUITAN | room::SANMA) == 0
    }

    /// The events split into hands, each starting with its `Init`.
    pub fn hands(&self) -> Vec<&[LogEvent]> {
        let starts: Vec<usize> = self
            .events
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e, LogEvent::Init(_)))
            .map(|(i, _)| i)
            .collect();
        starts
            .iter()
            .enumerate()
            .map(|(k, &s)| {
                let end = starts.get(k + 1).copied().unwrap_or(self.events.len());
                &self.events[s..end]
            })
            .collect()
    }
}

/// Parses a whole mjlog document.
pub fn parse(input: &str) -> Result<Game, LogError> {
    let mut game = Game {
        room: 0,
        events: Vec::new(),
    };
    for el in xml::elements(input)? {
        if let Some(ev) =
            event(&el, &mut game.room).map_err(|e| LogError(format!("<{}>: {}", el.name, e.0)))?
        {
            game.events.push(ev);
        }
    }
    if !game.events.iter().any(|e| matches!(e, LogEvent::Init(_))) {
        return err("no hands in log");
    }
    Ok(game)
}

fn event(el: &Element, room: &mut u32) -> Result<Option<LogEvent>, LogError> {
    let name = el.name;
    let first = name.as_bytes()[0];
    let rest = &name[1..];
    if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
        let tile = tile_id(rest)?;
        if let Some(seat) = b"TUVW".iter().position(|&c| c == first) {
            return Ok(Some(LogEvent::Draw {
                seat: seat as u8,
                tile,
            }));
        }
        if let Some(seat) = b"DEFG".iter().position(|&c| c == first) {
            return Ok(Some(LogEvent::Discard {
                seat: seat as u8,
                tile,
            }));
        }
        return err("unknown element");
    }
    Ok(Some(match name {
        "GO" => {
            *room = num(req(el, "type")?)?;
            return Ok(None);
        }
        "INIT" => LogEvent::Init(init(el)?),
        "N" => LogEvent::Call {
            seat: seat(req(el, "who")?)?,
            call: decode_meld(num(req(el, "m")?)?)?,
        },
        "REACH" => LogEvent::Riichi {
            seat: seat(req(el, "who")?)?,
            step: num(req(el, "step")?)?,
        },
        "DORA" => LogEvent::Dora {
            indicator: tile_id(req(el, "hai")?)?,
        },
        "AGARI" => LogEvent::Agari(Box::new(agari(el)?)),
        "RYUUKYOKU" => LogEvent::Ryuukyoku(Box::new(ryuukyoku(el)?)),
        _ => return Ok(None),
    }))
}

fn req<'a>(el: &Element<'a>, key: &str) -> Result<&'a str, LogError> {
    el.attr(key)
        .ok_or_else(|| LogError(format!("missing attribute {key}")))
}

fn num<T: std::str::FromStr>(s: &str) -> Result<T, LogError> {
    s.trim()
        .parse()
        .map_err(|_| LogError(format!("bad number {s:?}")))
}

fn nums<T: std::str::FromStr>(s: &str) -> Result<Vec<T>, LogError> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',').map(num).collect()
}

fn tile_id(s: &str) -> Result<TileId, LogError> {
    let t: u8 = num(s)?;
    if t >= 136 {
        return err(format!("tile id {t} out of range"));
    }
    Ok(t)
}

fn tile_ids(s: &str) -> Result<Vec<TileId>, LogError> {
    if s.is_empty() {
        return Ok(Vec::new());
    }
    s.split(',').map(tile_id).collect()
}

fn seat(s: &str) -> Result<u8, LogError> {
    let v: u8 = num(s)?;
    if v >= 4 {
        return err(format!("seat {v} out of range"));
    }
    Ok(v)
}

fn four(v: &[i32], what: &str) -> Result<[i32; 4], LogError> {
    v.try_into()
        .map_err(|_| LogError(format!("{what}: expected 4 values")))
}

/// `sc="s0,d0,s1,d1,..."` in hundreds: the deltas, in points.
fn deltas(el: &Element) -> Result<[i32; 4], LogError> {
    let sc: Vec<i32> = nums(req(el, "sc")?)?;
    if sc.len() != 8 {
        return err("sc: expected 8 values");
    }
    Ok([sc[1] * 100, sc[3] * 100, sc[5] * 100, sc[7] * 100])
}

/// `owari="s0,uma0,s1,uma1,..."`: the final scores, in points.
fn owari(el: &Element) -> Result<Option<[i32; 4]>, LogError> {
    let Some(s) = el.attr("owari") else {
        return Ok(None);
    };
    let v: Vec<f64> = nums(s)?;
    if v.len() != 8 {
        return err("owari: expected 8 values");
    }
    Ok(Some([0, 2, 4, 6].map(|i| (v[i] as i32) * 100)))
}

fn ba(el: &Element) -> Result<(u8, u8), LogError> {
    let v: Vec<u8> = nums(req(el, "ba")?)?;
    match v[..] {
        [h, s] => Ok((h, s)),
        _ => err("ba: expected 2 values"),
    }
}

fn init(el: &Element) -> Result<Init, LogError> {
    let seed: Vec<u8> = nums(req(el, "seed")?)?;
    if seed.len() != 6 {
        return err("seed: expected 6 values");
    }
    let ten: Vec<i32> = nums(req(el, "ten")?)?;
    let scores = four(&ten, "ten")?.map(|t| t * 100);
    let mut hands: [Vec<TileId>; 4] = Default::default();
    for (i, h) in hands.iter_mut().enumerate() {
        *h = tile_ids(req(el, &format!("hai{i}"))?)?;
        if h.len() != 13 {
            return err(format!("hai{i}: expected 13 tiles"));
        }
    }
    Ok(Init {
        round: seed[0],
        honba: seed[1],
        sticks: seed[2],
        dice: [seed[3], seed[4]],
        dora_indicator: tile_id(&seed[5].to_string())?,
        scores,
        dealer: seat(req(el, "oya")?)?,
        hands,
    })
}

fn agari(el: &Element) -> Result<Agari, LogError> {
    let (honba, sticks) = ba(el)?;
    let ten: Vec<u32> = nums(req(el, "ten")?)?;
    let [fu, points, limit] = ten[..] else {
        return err("ten: expected 3 values");
    };
    let yaku_flat: Vec<u8> = nums(el.attr("yaku").unwrap_or(""))?;
    if yaku_flat.len() % 2 != 0 {
        return err("yaku: odd length");
    }
    let melds = nums::<u32>(el.attr("m").unwrap_or(""))?
        .into_iter()
        .map(decode_meld)
        .collect::<Result<_, _>>()?;
    Ok(Agari {
        who: seat(req(el, "who")?)?,
        from: seat(req(el, "fromWho")?)?,
        honba,
        sticks,
        hand: tile_ids(req(el, "hai")?)?,
        melds,
        win_tile: tile_id(req(el, "machi")?)?,
        fu,
        points,
        limit: limit as u8,
        yaku: yaku_flat.chunks(2).map(|c| (c[0], c[1])).collect(),
        yakuman: nums(el.attr("yakuman").unwrap_or(""))?,
        dora_indicators: tile_ids(el.attr("doraHai").unwrap_or(""))?,
        ura_indicators: tile_ids(el.attr("doraHaiUra").unwrap_or(""))?,
        pao: el.attr("paoWho").map(seat).transpose()?,
        deltas: deltas(el)?,
        owari: owari(el)?,
    })
}

fn ryuukyoku(el: &Element) -> Result<Ryuukyoku, LogError> {
    let (honba, sticks) = ba(el)?;
    let kind = match el.attr("type") {
        None => DrawKind::Exhaustive,
        Some("yao9") => DrawKind::Kyuushu,
        Some("reach4") => DrawKind::SuuchaRiichi,
        Some("ron3") => DrawKind::Sanchahou,
        Some("kan4") => DrawKind::Suukaikan,
        Some("kaze4") => DrawKind::SuufonRenda,
        Some("nm") => DrawKind::NagashiMangan,
        Some(t) => return err(format!("unknown draw type {t:?}")),
    };
    let shown = [0, 1, 2, 3].map(|i| el.attr(&format!("hai{i}")).is_some());
    Ok(Ryuukyoku {
        kind,
        honba,
        sticks,
        deltas: deltas(el)?,
        shown,
        owari: owari(el)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn red_fives_have_their_own_codes() {
        assert_eq!(tile_of(16, true).code(), 34);
        assert_eq!(tile_of(17, true).code(), 4);
        assert_eq!(tile_of(16, false).code(), 4);
        assert_eq!(tile_of(52, true).code(), 35);
        assert_eq!(tile_of(88, true).code(), 36);
        assert_eq!(tile_of(135, true).code(), 33);
    }

    // Melds from real logs, checked by hand.
    #[test]
    fn decodes_chi() {
        // 47335 >> 10 = 46: called index 1, t = 15, so the lowest kind is
        // (15 / 7) * 9 + 15 % 7 = 19 (2s): a 234s chi.
        let c = decode_meld(47335).unwrap();
        assert_eq!(c.kind, CallKind::Chi);
        assert_eq!(
            c.tiles.iter().map(|t| t / 4).collect::<Vec<_>>(),
            [19, 20, 21]
        );
        assert_eq!(c.taken, Some(c.tiles[1]));
        assert_eq!(c.from, 3);
        assert_eq!(c.from_hand().len(), 2);
    }

    #[test]
    fn decodes_pon_and_kakan() {
        // 49259 = pon of kind 32 (hatsu): 49259 >> 9 = 96 -> kind 32, called 0.
        let p = decode_meld(49259).unwrap();
        assert_eq!(p.kind, CallKind::Pon);
        assert_eq!(p.tiles.len(), 3);
        assert!(p.tiles.iter().all(|t| t / 4 == 32));
        let k = decode_meld(49259 & !0x8 | 0x10).unwrap();
        assert_eq!(k.kind, CallKind::Kakan);
        assert_eq!(k.tiles.len(), 4);
        assert_eq!(k.from_hand(), vec![k.taken.unwrap()]);
        assert!(!p.tiles.contains(&k.taken.unwrap()));
    }

    #[test]
    fn decodes_kans() {
        // Ankan of tile id 100 (kind 25): m = 100 << 8.
        let a = decode_meld(100 << 8).unwrap();
        assert_eq!(a.kind, CallKind::Ankan);
        assert_eq!(a.tiles, vec![100, 101, 102, 103]);
        assert_eq!(a.taken, None);
        let d = decode_meld(101 << 8 | 2).unwrap();
        assert_eq!(d.kind, CallKind::Daiminkan);
        assert_eq!(d.taken, Some(101));
        assert_eq!(d.from_hand(), vec![100, 102, 103]);
    }

    #[test]
    fn parses_a_small_log() {
        let g = parse(
            r#"<mjloggm ver="2.3"><GO type="169" lobby="0"/><TAIKYOKU oya="0"/>
            <INIT seed="0,0,0,5,4,105" ten="250,250,250,250" oya="0" hai0="72,58,22,90,91,2,45,98,63,62,94,50,113" hai1="101,14,54,61,81,12,123,120,110,92,73,65,38" hai2="100,43,60,109,28,119,59,77,103,42,106,95,34" hai3="83,51,107,93,97,118,112,37,32,132,31,84,102"/>
            <T25/><D25/><REACH who="0" step="1"/>
            <AGARI ba="0,2" hai="17,22,25,40,45,50,58,62,66,86,87,90,94,98" machi="66" ten="30,5800,0" yaku="1,1,7,1,8,1,53,0" doraHai="105" doraHaiUra="78" who="0" fromWho="2" sc="240,78,250,0,240,-58,250,0" />
            <RYUUKYOKU type="yao9" ba="0,1" sc="409,0,228,0,170,0,183,0" hai1="41,43"/>
            </mjloggm>"#,
        )
        .unwrap();
        assert!(g.is_tenhou_ranked_rules() && g.hanchan());
        assert_eq!(g.hands().len(), 1);
        let LogEvent::Init(init) = &g.events[0] else {
            panic!()
        };
        assert_eq!(init.scores, [25_000; 4]);
        assert_eq!(init.dora_indicator, 105);
        assert_eq!(g.events[1], LogEvent::Draw { seat: 0, tile: 25 });
        assert_eq!(g.events[2], LogEvent::Discard { seat: 0, tile: 25 });
        let LogEvent::Agari(a) = &g.events[4] else {
            panic!()
        };
        assert_eq!(
            (a.who, a.from, a.fu, a.points, a.han()),
            (0, 2, 30, 5800, 3)
        );
        assert_eq!(a.deltas, [7800, 0, -5800, 0]);
        let LogEvent::Ryuukyoku(r) = &g.events[5] else {
            panic!()
        };
        assert_eq!(r.kind, DrawKind::Kyuushu);
        assert_eq!(r.shown, [false, true, false, false]);
    }
}

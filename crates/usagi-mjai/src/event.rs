//! Mjai messages.
//!
//! Tiles are strings: `1m`..`9m`, `1p`..`9p`, `1s`..`9s`, red fives `5mr`,
//! `5pr`, `5sr`, winds `E S W N`, dragons `P` (white), `F` (green), `C`
//! (red), and `?` for a tile the receiver may not see.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use usagi_core::Tile;

/// A tile as Mjai writes it, or `?`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Pai(pub Option<Tile>);

const HONORS: [&str; 7] = ["E", "S", "W", "N", "P", "F", "C"];

impl Pai {
    pub const UNKNOWN: Pai = Pai(None);

    pub fn tile(self) -> Option<Tile> {
        self.0
    }
}

impl From<Tile> for Pai {
    fn from(t: Tile) -> Self {
        Pai(Some(t))
    }
}

impl fmt::Display for Pai {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(t) = self.0 else {
            return f.write_str("?");
        };
        let k = t.kind();
        if k >= 27 {
            return f.write_str(HONORS[(k - 27) as usize]);
        }
        let suit = ['m', 'p', 's'][(k / 9) as usize];
        write!(f, "{}{}", k % 9 + 1, suit)?;
        if t.is_red() {
            f.write_str("r")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for Pai {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "?" {
            return Ok(Pai::UNKNOWN);
        }
        if let Some(i) = HONORS.iter().position(|&h| h == s) {
            return Ok(Tile::from_kind(27 + i as u8).into());
        }
        let b = s.as_bytes();
        let bad = || format!("bad tile {s:?}");
        if !(b.len() == 2 || b.len() == 3 && b[2] == b'r') {
            return Err(bad());
        }
        let n = b[0].wrapping_sub(b'1');
        let suit = match b[1] {
            b'm' => 0,
            b'p' => 1,
            b's' => 2,
            _ => return Err(bad()),
        };
        if n > 8 {
            return Err(bad());
        }
        if b.len() == 3 {
            if n != 4 {
                return Err(bad());
            }
            return Ok(Tile::from_code(34 + suit).unwrap().into());
        }
        Ok(Tile::from_kind(suit * 9 + n).into())
    }
}

impl Serialize for Pai {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Pai {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// One Mjai event or bot response.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    StartGame {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<u8>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        names: Vec<String>,
    },
    StartKyoku {
        bakaze: Pai,
        dora_marker: Pai,
        /// 1..=4.
        kyoku: u8,
        honba: u8,
        kyotaku: u8,
        oya: u8,
        scores: [i32; 4],
        tehais: [Vec<Pai>; 4],
    },
    Tsumo {
        actor: u8,
        pai: Pai,
    },
    Dahai {
        actor: u8,
        pai: Pai,
        #[serde(default)]
        tsumogiri: bool,
    },
    Chi {
        actor: u8,
        target: u8,
        pai: Pai,
        consumed: Vec<Pai>,
    },
    Pon {
        actor: u8,
        target: u8,
        pai: Pai,
        consumed: Vec<Pai>,
    },
    Daiminkan {
        actor: u8,
        target: u8,
        pai: Pai,
        consumed: Vec<Pai>,
    },
    Ankan {
        actor: u8,
        consumed: Vec<Pai>,
    },
    Kakan {
        actor: u8,
        pai: Pai,
        consumed: Vec<Pai>,
    },
    Dora {
        dora_marker: Pai,
    },
    Reach {
        actor: u8,
    },
    ReachAccepted {
        actor: u8,
    },
    Hora {
        actor: u8,
        target: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pai: Option<Pai>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deltas: Option<[i32; 4]>,
    },
    Ryukyoku {
        /// Set when a bot declares kyuushu kyuuhai.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        actor: Option<u8>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deltas: Option<[i32; 4]>,
    },
    EndKyoku,
    EndGame,
    /// A bot's "no action".
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_round_trip() {
        for code in 0..37u8 {
            let p = Pai::from(Tile::from_code(code).unwrap());
            let s = p.to_string();
            assert_eq!(s.parse::<Pai>().unwrap(), p, "{s}");
        }
        assert_eq!(Pai::from(Tile::from_code(34).unwrap()).to_string(), "5mr");
        assert_eq!(Pai::from(Tile::from_kind(31)).to_string(), "P");
        assert_eq!("?".parse::<Pai>().unwrap(), Pai::UNKNOWN);
        for bad in ["", "0m", "5zr", "1mr", "x", "10m"] {
            assert!(bad.parse::<Pai>().is_err(), "{bad}");
        }
    }

    #[test]
    fn events_use_mjai_json() {
        let e: Event =
            serde_json::from_str(r#"{"type":"dahai","actor":1,"pai":"5pr","tsumogiri":false}"#)
                .unwrap();
        assert_eq!(
            e,
            Event::Dahai {
                actor: 1,
                pai: Tile::from_code(35).unwrap().into(),
                tsumogiri: false
            }
        );
        assert_eq!(
            serde_json::to_string(&Event::None).unwrap(),
            r#"{"type":"none"}"#
        );
        assert_eq!(
            serde_json::to_string(&Event::ReachAccepted { actor: 2 }).unwrap(),
            r#"{"type":"reach_accepted","actor":2}"#
        );
    }
}

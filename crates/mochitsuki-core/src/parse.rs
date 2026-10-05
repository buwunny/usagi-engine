//! Hand strings like `123m456p789s11z`.
//!
//! This is the notation every Mahjong tool uses, and the one all tests in
//! this repository are written in:
//!
//! - digits followed by a suit letter: `m` manzu, `p` pinzu, `s` souzu,
//!   `z` honors (1-4 = E S W N, 5-7 = White Green Red),
//! - `0` is a red five (`0m`, `0p`, `0s`); `0z` is an error,
//! - spaces are ignored, so `123m 456p` is fine.
//!
//! Example: `"340m"` is 3m, 4m and a red 5m.

use std::fmt;

use crate::counts::Counts;
use crate::tile::Tile;

/// Everything that can go wrong while parsing a hand string.
///
/// An enum like this, instead of a `String` message, lets callers `match`
/// on what went wrong. Implementing `Display` gives it a readable message
/// and implementing `std::error::Error` lets it work with `?` and
/// `Box<dyn Error>` later on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ParseError {
    /// A character that is not a digit, suit letter or space.
    UnexpectedChar(char),
    /// The string ended with digits that had no suit letter after them,
    /// as in `"123m45"`.
    MissingSuit,
    /// A suit letter with no digits before it, as in `"m"` or `"123mp"`.
    EmptyGroup,
    /// An honor digit outside 1..=7 (`0z`, `8z`, `9z`).
    InvalidHonor(u8),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnexpectedChar(c) => write!(f, "unexpected character {c:?}"),
            ParseError::MissingSuit => write!(f, "digits at the end have no suit letter"),
            ParseError::EmptyGroup => write!(f, "suit letter with no digits before it"),
            ParseError::InvalidHonor(d) => write!(f, "invalid honor digit {d} (must be 1..=7)"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses a hand string into tiles, in the order they appear.
///
/// It only checks notation, not Mahjong legality: `"11111m"` parses fine
/// (five 1m), and it is [`Counts::add`] that objects later.
pub fn parse_tiles(s: &str) -> Result<Vec<Tile>, ParseError> {
    use crate::tile::Suit;
    let mut tiles = Vec::new();
    let mut pending: Vec<u8> = Vec::new();
    for ch in s.chars() {
        match ch {
            '0'..='9' => pending.push(ch as u8 - b'0'),
            'm' | 'p' | 's' | 'z' => {
                if pending.is_empty() {
                    return Err(ParseError::EmptyGroup);
                }
                let suit = match ch {
                    'm' => Suit::Man,
                    'p' => Suit::Pin,
                    's' => Suit::Sou,
                    _ => Suit::Honor,
                };
                for &d in &pending {
                    let t = if suit == Suit::Honor {
                        if !(1..=7).contains(&d) {
                            return Err(ParseError::InvalidHonor(d));
                        }
                        Tile::from_kind(27 + d - 1)
                    } else if d == 0 {
                        Tile::red_five(suit)
                    } else {
                        Tile::from_kind(suit.first_kind() + d - 1)
                    };
                    tiles.push(t);
                }
                pending.clear();
            }
            ' ' => {}
            other => return Err(ParseError::UnexpectedChar(other)),
        }
    }
    if !pending.is_empty() {
        return Err(ParseError::MissingSuit);
    }
    Ok(tiles)
}

/// Formats tiles in canonical form: suits in the order m, p, s, z; numbers
/// ascending inside each suit; a red five is written `0` and placed just
/// before the plain 5s. Empty input gives an empty string.
///
/// `format_tiles(&tiles_of("5m0m1m3z1p"))` is `"105m1p3z"`.
pub fn format_tiles(tiles: &[Tile]) -> String {
    let mut v = tiles.to_vec();
    // key: kind*2, minus 1 for a red five so it sorts just before plain 5s
    v.sort_by_key(|t| t.kind() as u16 * 2 + if t.is_red() { 0 } else { 1 });
    let mut out = String::new();
    for (i, t) in v.iter().enumerate() {
        let d = if t.is_red() { 0 } else { t.number() };
        out.push((b'0' + d) as char);
        if i + 1 == v.len() || v[i + 1].suit() != t.suit() {
            out.push(t.suit().letter());
        }
    }
    out
}

/// Test helper: parses or panics. Lets tests write `tiles_of("123m")`.
pub fn tiles_of(s: &str) -> Vec<Tile> {
    match parse_tiles(s) {
        Ok(tiles) => tiles,
        Err(e) => panic!("bad hand string {s:?}: {e}"),
    }
}

/// Test helper: parses into [`Counts`] or panics.
pub fn counts_of(s: &str) -> Counts {
    Counts::from_tiles(&tiles_of(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tile::Suit;
    use proptest::prelude::*;

    // cargo test -p mochitsuki-core parse::

    #[test]
    fn parses_simple_hand() {
        let tiles = parse_tiles("123m456p789s11z").unwrap();
        let codes: Vec<u8> = tiles.iter().map(|t| t.code()).collect();
        assert_eq!(codes, vec![0, 1, 2, 12, 13, 14, 24, 25, 26, 27, 27]);
    }

    #[test]
    fn parses_red_fives_and_spaces() {
        let tiles = parse_tiles("0m 05p 0s").unwrap();
        assert_eq!(
            tiles,
            vec![
                Tile::red_five(Suit::Man),
                Tile::red_five(Suit::Pin),
                Tile::from_kind(13),
                Tile::red_five(Suit::Sou),
            ]
        );
    }

    #[test]
    fn errors() {
        assert_eq!(parse_tiles("123x"), Err(ParseError::UnexpectedChar('x')));
        assert_eq!(parse_tiles("123m45"), Err(ParseError::MissingSuit));
        assert_eq!(parse_tiles("123mp"), Err(ParseError::EmptyGroup));
        assert_eq!(parse_tiles("8z"), Err(ParseError::InvalidHonor(8)));
        assert_eq!(parse_tiles("0z"), Err(ParseError::InvalidHonor(0)));
        assert_eq!(parse_tiles(""), Ok(vec![]));
        // Errors must print something readable.
        assert!(!ParseError::MissingSuit.to_string().is_empty());
    }

    #[test]
    fn formats_canonically() {
        assert_eq!(format_tiles(&tiles_of("5m0m1m3z1p")), "105m1p3z");
        assert_eq!(
            format_tiles(&tiles_of("11z789s456p123m")),
            "123m456p789s11z"
        );
        assert_eq!(format_tiles(&[]), "");
    }

    #[test]
    fn counts_helper() {
        let c = counts_of("123m11z");
        assert_eq!(c.total(), 5);
        assert_eq!(c.get(27), 2);
    }

    // A property test: instead of one example, proptest generates hundreds
    // of random inputs and checks the property holds for all of them. When
    // one fails it shrinks it to the smallest failing input it can find.
    proptest! {
        #[test]
        fn format_then_parse_round_trips(codes in prop::collection::vec(0u8..37, 0..20)) {
            let tiles: Vec<Tile> = codes.iter().map(|&c| Tile::from_code(c).unwrap()).collect();
            let text = format_tiles(&tiles);
            let mut back = parse_tiles(&text).unwrap();
            let mut original = tiles.clone();
            original.sort();
            back.sort();
            prop_assert_eq!(back, original, "formatted as {:?}", text);
        }

        #[test]
        fn formatting_is_canonical(codes in prop::collection::vec(0u8..37, 0..20)) {
            // Formatting the same multiset in any order gives the same string.
            let tiles: Vec<Tile> = codes.iter().map(|&c| Tile::from_code(c).unwrap()).collect();
            let mut reversed = tiles.clone();
            reversed.reverse();
            prop_assert_eq!(format_tiles(&tiles), format_tiles(&reversed));
        }
    }
}

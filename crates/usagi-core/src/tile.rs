//! Tiles.
//!
//! A Riichi set has 136 tiles: 34 *kinds*, four copies of each. Three of
//! the 5s (one 5m, one 5p, one 5s) are painted red ("aka dora") and are
//! worth an extra han, so a red five has to stay distinguishable from a
//! plain one even though it plays exactly like one.
//!
//! ## Encoding
//!
//! A [`Tile`] is one byte, its *code*:
//!
//! | Code     | Tiles                                            |
//! |----------|--------------------------------------------------|
//! | 0..=8    | 1m..9m (manzu, "characters")                     |
//! | 9..=17   | 1p..9p (pinzu, "circles")                        |
//! | 18..=26  | 1s..9s (souzu, "bamboo")                         |
//! | 27..=30  | East, South, West, North (winds), written 1z..4z |
//! | 31..=33  | White, Green, Red (dragons), written 5z..7z      |
//! | 34       | red 5m, written 0m                               |
//! | 35       | red 5p, written 0p                               |
//! | 36       | red 5s, written 0s                               |
//!
//! The *kind* of a tile is its code with red fives folded back onto the
//! plain five (34 -> 4, 35 -> 13, 36 -> 22). Kinds are always 0..=33 and
//! are what hand logic (shanten, waits, yaku) works with. Codes are what
//! the wall and discards store, so scoring can still count red fives.

use std::fmt;

/// Number of distinct tile kinds (0..=33).
pub const NUM_KINDS: usize = 34;

/// Number of distinct tile codes (0..=36, red fives included).
pub const NUM_CODES: usize = 37;

/// Kind constants for the honors, so code reads `kind == EAST`
/// instead of `kind == 27`.
pub const EAST: u8 = 27;
pub const SOUTH: u8 = 28;
pub const WEST: u8 = 29;
pub const NORTH: u8 = 30;
pub const HAKU: u8 = 31; // white dragon
pub const HATSU: u8 = 32; // green dragon
pub const CHUN: u8 = 33; // red dragon

/// The three number suits plus honors.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Suit {
    Man,
    Pin,
    Sou,
    Honor,
}

impl Suit {
    /// The letter used in hand strings: `m`, `p`, `s` or `z`.
    pub fn letter(self) -> char {
        match self {
            Suit::Man => 'm',
            Suit::Pin => 'p',
            Suit::Sou => 's',
            Suit::Honor => 'z',
        }
    }

    /// The kind of this suit's "1" tile: 0 for Man, 9 for Pin, 18 for Sou,
    /// 27 for Honor. Handy for turning (suit, number) into a kind.
    pub fn first_kind(self) -> u8 {
        match self {
            Suit::Man => 0,
            Suit::Pin => 9,
            Suit::Sou => 18,
            Suit::Honor => 27,
        }
    }
}

/// One physical tile. See the module docs for the encoding.
///
/// `#[derive(...)]` asks the compiler to write some standard trait impls
/// for you. `Copy` means a `Tile` is copied (like an integer) instead of
/// moved when you pass it around, which is what you want for a 1-byte value.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Tile(u8);

impl Tile {
    /// Builds a tile from a code, or returns `None` if `code > 36`.
    pub const fn from_code(code: u8) -> Option<Tile> {
        if code <= 36 { Some(Tile(code)) } else { None }
    }

    /// Builds a plain (non-red) tile of the given kind. Panics if `kind > 33`.
    pub const fn from_kind(kind: u8) -> Tile {
        assert!(kind <= 33);
        Tile(kind)
    }

    /// The red five of a number suit. Panics for `Suit::Honor`.
    pub const fn red_five(suit: Suit) -> Tile {
        match suit {
            Suit::Man => Tile(34),
            Suit::Pin => Tile(35),
            Suit::Sou => Tile(36),
            Suit::Honor => panic!("no red honor"),
        }
    }

    /// The raw code, 0..=36. (Given: this is how other modules read the byte.)
    pub const fn code(self) -> u8 {
        self.0
    }

    /// The kind, 0..=33, with red fives folded onto plain fives.
    pub const fn kind(self) -> u8 {
        match self.0 {
            34 => 4,
            35 => 13,
            36 => 22,
            k => k,
        }
    }

    /// True for the three red fives.
    pub const fn is_red(self) -> bool {
        self.0 >= 34
    }

    /// Which suit the tile belongs to.
    pub fn suit(self) -> Suit {
        match self.kind() / 9 {
            0 => Suit::Man,
            1 => Suit::Pin,
            2 => Suit::Sou,
            _ => Suit::Honor,
        }
    }

    /// The number printed on the tile: 1..=9 for suits, 1..=7 for honors
    /// (East = 1 ... Red dragon = 7, matching the `z` notation).
    pub fn number(self) -> u8 {
        self.kind() - self.suit().first_kind() + 1
    }

    /// Winds and dragons.
    pub fn is_honor(self) -> bool {
        self.kind() >= 27
    }

    /// 1s and 9s of the number suits (not honors).
    pub fn is_terminal(self) -> bool {
        !self.is_honor() && (self.number() == 1 || self.number() == 9)
    }

    /// Terminal or honor. Japanese: *yaochuuhai*. Kokushi, chanta and
    /// tanyao are all defined in terms of these.
    pub fn is_terminal_or_honor(self) -> bool {
        self.is_honor() || self.is_terminal()
    }

    /// 2 through 8 of a number suit. Japanese: *chunchanpai*.
    pub fn is_simple(self) -> bool {
        !self.is_terminal_or_honor()
    }

    pub fn is_wind(self) -> bool {
        (EAST..=NORTH).contains(&self.kind())
    }

    pub fn is_dragon(self) -> bool {
        self.kind() >= HAKU
    }
}

/// Given a dora *indicator* kind, returns the kind that is actually dora.
///
/// The dora is the "next" tile after the indicator:
/// - number suits go up by one and wrap 9 -> 1 within the suit (9m -> 1m),
/// - winds cycle East -> South -> West -> North -> East,
/// - dragons cycle White -> Green -> Red -> White.
pub fn dora_from_indicator(indicator_kind: u8) -> u8 {
    let k = indicator_kind;
    if k < 27 {
        let base = k / 9 * 9;
        base + (k - base + 1) % 9
    } else if k <= NORTH {
        EAST + (k - EAST + 1) % 4
    } else {
        HAKU + (k - HAKU + 1) % 3
    }
}

/// Prints `5m`, `0p` (red five), `3z` (West) and so on.
impl fmt::Display for Tile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = if self.is_red() { 0 } else { self.number() };
        write!(f, "{}{}", n, self.suit().letter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Run just these with:  cargo test -p usagi-core tile::

    #[test]
    fn from_code_range() {
        assert_eq!(Tile::from_code(0).map(Tile::code), Some(0));
        assert_eq!(Tile::from_code(36).map(Tile::code), Some(36));
        assert_eq!(Tile::from_code(37), None);
        assert_eq!(Tile::from_code(255), None);
    }

    #[test]
    fn kinds_and_reds() {
        for code in 0..=33 {
            let t = Tile::from_code(code).unwrap();
            assert_eq!(t.kind(), code);
            assert!(!t.is_red());
        }
        assert_eq!(Tile::red_five(Suit::Man).kind(), 4);
        assert_eq!(Tile::red_five(Suit::Pin).kind(), 13);
        assert_eq!(Tile::red_five(Suit::Sou).kind(), 22);
        assert!(Tile::red_five(Suit::Sou).is_red());
        assert_eq!(Tile::from_kind(22).code(), 22);
    }

    #[test]
    fn from_kind_rejects_red_codes() {
        Tile::from_kind(33); // fine
        let result = std::panic::catch_unwind(|| Tile::from_kind(34));
        assert!(result.is_err(), "from_kind(34) must panic");
    }

    #[test]
    fn suit_and_number() {
        let t = Tile::from_kind(0);
        assert_eq!((t.suit(), t.number()), (Suit::Man, 1));
        let t = Tile::from_kind(17);
        assert_eq!((t.suit(), t.number()), (Suit::Pin, 9));
        let t = Tile::red_five(Suit::Sou);
        assert_eq!((t.suit(), t.number()), (Suit::Sou, 5));
        let t = Tile::from_kind(CHUN);
        assert_eq!((t.suit(), t.number()), (Suit::Honor, 7));
        assert_eq!(Suit::Pin.first_kind(), 9);
        assert_eq!(Suit::Honor.letter(), 'z');
    }

    #[test]
    fn categories() {
        let one_m = Tile::from_kind(0);
        let five_p = Tile::from_kind(13);
        let nine_s = Tile::from_kind(26);
        let east = Tile::from_kind(EAST);
        let haku = Tile::from_kind(HAKU);

        assert!(one_m.is_terminal() && !one_m.is_honor() && !one_m.is_simple());
        assert!(nine_s.is_terminal_or_honor());
        assert!(five_p.is_simple() && !five_p.is_terminal_or_honor());
        assert!(Tile::red_five(Suit::Man).is_simple());
        assert!(east.is_honor() && east.is_wind() && !east.is_dragon());
        assert!(haku.is_dragon() && !haku.is_terminal() && haku.is_terminal_or_honor());
    }

    #[test]
    fn dora_wraps() {
        assert_eq!(dora_from_indicator(0), 1); // 1m -> 2m
        assert_eq!(dora_from_indicator(8), 0); // 9m -> 1m
        assert_eq!(dora_from_indicator(17), 9); // 9p -> 1p
        assert_eq!(dora_from_indicator(26), 18); // 9s -> 1s
        assert_eq!(dora_from_indicator(NORTH), EAST);
        assert_eq!(dora_from_indicator(SOUTH), WEST);
        assert_eq!(dora_from_indicator(CHUN), HAKU);
        assert_eq!(dora_from_indicator(HAKU), HATSU);
    }

    #[test]
    fn display() {
        assert_eq!(Tile::from_kind(0).to_string(), "1m");
        assert_eq!(Tile::from_kind(13).to_string(), "5p");
        assert_eq!(Tile::red_five(Suit::Pin).to_string(), "0p");
        assert_eq!(Tile::from_kind(EAST).to_string(), "1z");
        assert_eq!(Tile::from_kind(CHUN).to_string(), "7z");
    }
}

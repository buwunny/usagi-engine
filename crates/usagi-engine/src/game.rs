//! Turn flow: `new`, `legal_actions` and `step`.
//!
//! Every state change goes through [`GameState::step`], and `step` accepts
//! exactly the actions [`GameState::legal_actions`] lists. Rule details
//! follow `docs/reference/tenhou-rules.md`.

use usagi_core::hand::{Meld, MeldKind};
use usagi_core::score::{Payment, WinResult, payment, score, score_deltas};
use usagi_core::shanten::shanten;
use usagi_core::tile::{EAST, NUM_KINDS};
use usagi_core::waits::waits;
use usagi_core::yaku::{Riichi, WinContext, Yaku};
use usagi_core::{Counts, Suit, Tile};

use crate::action::{Action, ActionList};
use crate::phase::{NO_KIND, Phase};
use crate::rules::Rules;
use crate::state::{
    GameState, NO_RIICHI, NO_SEAT, NO_TILE, PackedMeld, PlayerState, Round, discard_bits,
    player_flags as pf,
};
use crate::wall::{self, DORA_START, LIVE_WALL_END, RINSHAN_START, URA_START, WALL_SIZE};

/// Why `step` refused an action.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StepError {
    /// It isn't this seat's decision right now.
    NotYourTurn,
    /// The action isn't in `legal_actions(seat)`.
    Illegal,
    /// The hand or game is over.
    NotPlaying,
}

/// Why a hand ended without a winner before the wall ran out.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum AbortKind {
    KyuushuKyuuhai,
    SuufonRenda,
    SuuchaRiichi,
    Suukaikan,
    Sanchahou,
}

/// What happened during a step, in order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Event {
    RoundStarted {
        round: Round,
        dealer: u8,
        dora_indicator: Tile,
        scores: [i32; 4],
    },
    Draw {
        seat: u8,
        tile: Tile,
        rinshan: bool,
    },
    Discard {
        seat: u8,
        tile: Tile,
        tsumogiri: bool,
        riichi: bool,
    },
    RiichiAccepted {
        seat: u8,
    },
    /// Chi, pon or daiminkan on `from`'s discard `tile`.
    Call {
        seat: u8,
        from: u8,
        tile: Tile,
        action: Action,
    },
    /// Ankan or kakan.
    Kan {
        seat: u8,
        action: Action,
    },
    NewDora {
        indicator: Tile,
    },
    Win {
        seat: u8,
        /// `NO_SEAT` for tsumo.
        from: u8,
        tile: Tile,
        han: u8,
        fu: u8,
        yakuman: u8,
        /// Points paid for the hand itself (no honba, no sticks).
        points: u32,
    },
    ExhaustiveDraw {
        /// Bit `i` set if seat `i` was tenpai.
        tenpai: u8,
        /// Bit `i` set if seat `i` scored nagashi mangan.
        nagashi: u8,
    },
    AbortiveDraw {
        kind: AbortKind,
    },
    /// Score changes from the hand that just ended (sticks included).
    RoundEnded {
        deltas: [i32; 4],
    },
    GameEnded {
        scores: [i32; 4],
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HandEnd {
    Win,
    Draw,
    Abort,
}

/// The 13 terminal and honor kinds.
const YAOCHUU: [u8; 13] = [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];

fn bit(seat: u8) -> u8 {
    1 << seat
}

fn is_five(kind: u8) -> bool {
    kind < 27 && kind % 9 == 4
}

fn red_bit(kind: u8) -> u8 {
    1 << (kind / 9)
}

fn red_tile(kind: u8) -> Tile {
    let suit = match kind / 9 {
        0 => Suit::Man,
        1 => Suit::Pin,
        _ => Suit::Sou,
    };
    Tile::red_five(suit)
}

fn tile_from_code(code: u8) -> Tile {
    Tile::from_code(code).expect("valid tile code")
}

/// splitmix64, for deriving per-hand wall seeds from the game seed.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl<R: Rules> GameState<R> {
    // ----------------------------------------------------------------
    // Construction and round setup
    // ----------------------------------------------------------------

    /// A new game at East 1 with a wall shuffled from `seed`, dealt, and
    /// the dealer's first tile drawn.
    pub fn new(seed: u64) -> Self {
        let mut g = Self::blank(seed);
        let wall = g.next_wall();
        g.deal(wall, &mut Vec::new());
        g
    }

    /// A new game at East 1 using the given wall for the first hand.
    pub fn with_wall(seed: u64, wall: [Tile; WALL_SIZE]) -> Self {
        let mut g = Self::blank(seed);
        g.deal(wall, &mut Vec::new());
        g
    }

    /// A game dropped into the middle of a match: round `round` with these
    /// scores and riichi sticks on the table, dealt from `wall`. Log replay
    /// starts each hand this way, so one bad hand doesn't hide the rest.
    pub fn with_hand(
        round: Round,
        riichi_sticks: u8,
        scores: [i32; 4],
        wall: [Tile; WALL_SIZE],
    ) -> Self {
        let mut g = Self::blank(0);
        g.round = round;
        g.riichi_sticks = riichi_sticks;
        g.scores = scores;
        g.deal(wall, &mut Vec::new());
        g
    }

    fn blank(seed: u64) -> Self {
        GameState {
            wall: [Tile::from_kind(0); WALL_SIZE],
            players: [PlayerState::EMPTY; 4],
            scores: [R::STARTING_POINTS; 4],
            seed,
            wall_pos: 0,
            dead_pos: 0,
            kan_count: 0,
            dora_revealed: 1,
            pending_dora: 0,
            round: Round::default(),
            riichi_sticks: 0,
            hand_number: 0,
            pao: [NO_SEAT; 4],
            phase: Phase::RoundEnd,
            rules: std::marker::PhantomData,
        }
    }

    fn next_wall(&self) -> [Tile; WALL_SIZE] {
        wall::shuffled(mix(self.seed ^ mix(self.hand_number as u64)), R::RED_FIVES)
    }

    /// Deals the next hand from the seeded wall. Only valid in `RoundEnd`.
    pub fn start_next_round(&mut self, events: &mut Vec<Event>) -> Result<(), StepError> {
        if self.phase != Phase::RoundEnd {
            return Err(StepError::NotPlaying);
        }
        let wall = self.next_wall();
        self.deal(wall, events);
        Ok(())
    }

    /// Deals the next hand from a given wall (log replay, duplicate
    /// matches). Only valid in `RoundEnd`.
    pub fn start_next_round_with_wall(
        &mut self,
        wall: [Tile; WALL_SIZE],
        events: &mut Vec<Event>,
    ) -> Result<(), StepError> {
        if self.phase != Phase::RoundEnd {
            return Err(StepError::NotPlaying);
        }
        self.deal(wall, events);
        Ok(())
    }

    /// Deals 13 tiles to each seat starting with the dealer (dealer gets
    /// `wall[0..13]`, the next seat `wall[13..26]`...), then the dealer draws.
    fn deal(&mut self, wall: [Tile; WALL_SIZE], events: &mut Vec<Event>) {
        self.wall = wall;
        self.players = [PlayerState::EMPTY; 4];
        self.wall_pos = 0;
        self.dead_pos = 0;
        self.kan_count = 0;
        self.dora_revealed = 1;
        self.pending_dora = 0;
        self.pao = [NO_SEAT; 4];
        let dealer = self.dealer();
        for i in 0..4u8 {
            let seat = (dealer + i) % 4;
            for _ in 0..13 {
                let t = self.wall[self.wall_pos as usize];
                self.wall_pos += 1;
                self.add_to_hand(seat, t);
            }
        }
        self.hand_number = self.hand_number.wrapping_add(1);
        events.push(Event::RoundStarted {
            round: self.round,
            dealer,
            dora_indicator: wall::dora_indicator(&self.wall, 0),
            scores: self.scores,
        });
        self.draw_live(dealer, events);
    }

    // ----------------------------------------------------------------
    // Small queries
    // ----------------------------------------------------------------

    pub fn dealer(&self) -> u8 {
        self.round.index % 4
    }

    pub fn round_wind(&self) -> u8 {
        EAST + self.round.index / 4
    }

    pub fn seat_wind(&self, seat: u8) -> u8 {
        EAST + (seat + 4 - self.dealer()) % 4
    }

    /// Tiles left in the live wall.
    pub fn tiles_left(&self) -> u8 {
        LIVE_WALL_END - self.kan_count - self.wall_pos
    }

    pub fn is_over(&self) -> bool {
        self.phase == Phase::GameEnd
    }

    /// Whose decision the game is waiting for, as a 4-bit mask. Zero in
    /// `RoundEnd` and `GameEnd`.
    pub fn waiting_on(&self) -> u8 {
        match self.phase {
            Phase::Turn { seat } | Phase::AfterCall { seat, .. } => bit(seat),
            Phase::CallWindow { pending, .. } | Phase::ChankanWindow { pending, .. } => pending,
            Phase::RoundEnd | Phase::GameEnd => 0,
        }
    }

    /// The face-up dora indicators.
    pub fn dora_indicators(&self) -> Vec<Tile> {
        (0..self.dora_revealed)
            .map(|i| wall::dora_indicator(&self.wall, i))
            .collect()
    }

    /// Final placement of each seat (0 = first). Ties go to the seat that
    /// was dealer earlier (lower seat index).
    pub fn ranks(&self) -> [u8; 4] {
        let mut order = [0u8, 1, 2, 3];
        order.sort_by_key(|&s| (-self.scores[s as usize], s));
        let mut ranks = [0u8; 4];
        for (place, &s) in order.iter().enumerate() {
            ranks[s as usize] = place as u8;
        }
        ranks
    }

    fn no_calls_yet(&self) -> bool {
        self.players.iter().all(|p| p.meld_count == 0)
    }

    /// The concealed tiles of `seat`, red fives included.
    pub fn concealed_tiles(&self, seat: u8) -> Vec<Tile> {
        let p = &self.players[seat as usize];
        let mut out = Vec::with_capacity(14);
        for k in 0..NUM_KINDS as u8 {
            let mut n = p.hand[k as usize];
            if n > 0 && is_five(k) && p.reds & red_bit(k) != 0 {
                out.push(red_tile(k));
                n -= 1;
            }
            for _ in 0..n {
                out.push(Tile::from_kind(k));
            }
        }
        out
    }

    pub fn melds(&self, seat: u8) -> Vec<Meld> {
        self.players[seat as usize]
            .melds()
            .iter()
            .map(|m| m.to_meld())
            .collect()
    }

    fn counts(&self, seat: u8) -> Counts {
        Counts(self.players[seat as usize].hand)
    }

    fn meld_count(&self, seat: u8) -> u8 {
        self.players[seat as usize].meld_count
    }

    // ----------------------------------------------------------------
    // Moving tiles
    // ----------------------------------------------------------------

    fn add_to_hand(&mut self, seat: u8, t: Tile) {
        let p = &mut self.players[seat as usize];
        let k = t.kind();
        debug_assert!(p.hand[k as usize] < 4);
        p.hand[k as usize] += 1;
        if t.is_red() {
            p.reds |= red_bit(k);
        }
    }

    /// Whether `seat` holds this exact tile (a red five is distinct).
    fn holds(&self, seat: u8, t: Tile) -> bool {
        let p = &self.players[seat as usize];
        let k = t.kind();
        let n = p.hand[k as usize];
        let red = is_five(k) && p.reds & red_bit(k) != 0;
        if t.is_red() { red } else { n > red as u8 }
    }

    fn remove_from_hand(&mut self, seat: u8, t: Tile) {
        debug_assert!(self.holds(seat, t), "seat {seat} doesn't hold {t}");
        let p = &mut self.players[seat as usize];
        let k = t.kind();
        p.hand[k as usize] -= 1;
        if t.is_red() {
            p.reds &= !red_bit(k);
        }
    }

    /// The tiles of `kind` a seat could give up, as distinct options
    /// (plain first, then red).
    fn distinct_tiles_of(&self, seat: u8, kind: u8) -> ([Tile; 2], usize) {
        let p = &self.players[seat as usize];
        let n = p.hand[kind as usize];
        let red = is_five(kind) && p.reds & red_bit(kind) != 0;
        let mut out = [Tile::from_kind(kind); 2];
        let mut len = 0;
        if n > red as u8 {
            len += 1;
        }
        if red {
            out[len] = red_tile(kind);
            len += 1;
        }
        (out, len)
    }

    fn draw_live(&mut self, seat: u8, events: &mut Vec<Event>) {
        let t = self.wall[self.wall_pos as usize];
        self.wall_pos += 1;
        self.add_to_hand(seat, t);
        let p = &mut self.players[seat as usize];
        p.drawn = t.code();
        p.flags &= !pf::RINSHAN;
        events.push(Event::Draw {
            seat,
            tile: t,
            rinshan: false,
        });
        self.phase = Phase::Turn { seat };
    }

    fn draw_rinshan(&mut self, seat: u8, events: &mut Vec<Event>) {
        let t = self.wall[(RINSHAN_START + self.dead_pos) as usize];
        self.dead_pos += 1;
        self.add_to_hand(seat, t);
        let p = &mut self.players[seat as usize];
        p.drawn = t.code();
        p.flags |= pf::RINSHAN;
        events.push(Event::Draw {
            seat,
            tile: t,
            rinshan: true,
        });
        self.phase = Phase::Turn { seat };
    }

    fn reveal_dora(&mut self, events: &mut Vec<Event>) {
        debug_assert!(self.dora_revealed < 5);
        self.dora_revealed += 1;
        events.push(Event::NewDora {
            indicator: self.wall[(DORA_START + self.dora_revealed - 1) as usize],
        });
    }

    fn reveal_pending_dora(&mut self, events: &mut Vec<Event>) {
        while self.pending_dora > 0 {
            self.pending_dora -= 1;
            self.reveal_dora(events);
        }
    }

    // ----------------------------------------------------------------
    // Winning
    // ----------------------------------------------------------------

    fn win_context(&self, seat: u8, win_tile: Tile, tsumo: bool, chankan: bool) -> WinContext {
        let p = &self.players[seat as usize];
        let riichi = if p.has(pf::DOUBLE_RIICHI) {
            Riichi::Double
        } else if p.in_riichi() {
            Riichi::Riichi
        } else {
            Riichi::None
        };
        let first_draw = tsumo && p.discard_len == 0 && self.no_calls_yet();
        let rinshan = tsumo && p.has(pf::RINSHAN);
        let last = self.tiles_left() == 0;
        let ura = if riichi != Riichi::None {
            (0..self.dora_revealed)
                .map(|i| self.wall[(URA_START + i) as usize])
                .collect()
        } else {
            Vec::new()
        };
        WinContext {
            win_tile,
            tsumo,
            riichi,
            ippatsu: p.has(pf::IPPATSU),
            rinshan,
            chankan,
            haitei: tsumo && last && !rinshan,
            houtei: !tsumo && last && !chankan,
            tenhou: first_draw && seat == self.dealer(),
            chiihou: first_draw && seat != self.dealer(),
            seat_wind: self.seat_wind(seat),
            round_wind: self.round_wind(),
            dora_indicators: self.dora_indicators(),
            ura_indicators: ura,
        }
    }

    /// Scores a win for `seat`: tsumo uses the hand as it is; ron adds `tile`.
    fn evaluate_win(&self, seat: u8, tile: Tile, tsumo: bool, chankan: bool) -> Option<WinResult> {
        let mut tiles = self.concealed_tiles(seat);
        if !tsumo {
            tiles.push(tile);
        }
        score(
            &tiles,
            &self.melds(seat),
            &self.win_context(seat, tile, tsumo, chankan),
        )
    }

    /// Whether `tile` completes `seat`'s 13-tile shape (ignoring yaku).
    fn completes(&self, seat: u8, kind: u8) -> bool {
        let mut c = self.counts(seat);
        if c.0[kind as usize] >= 4 {
            return false;
        }
        c.0[kind as usize] += 1;
        shanten(&c, self.meld_count(seat)) == -1
    }

    /// Whether `seat` may not ron right now: a wait is in its own river, or
    /// it passed a winning tile this go-around or since its riichi.
    pub fn is_furiten(&self, seat: u8) -> bool {
        let p = &self.players[seat as usize];
        if p.has(pf::TEMP_FURITEN) || p.has(pf::RIICHI_FURITEN) {
            return true;
        }
        let w = waits(&self.counts(seat), p.meld_count);
        p.discards()
            .iter()
            .any(|&d| w.contains(tile_from_code(d & discard_bits::CODE_MASK).kind()))
    }

    fn can_ron(&self, seat: u8, tile: Tile, chankan: bool) -> bool {
        self.completes(seat, tile.kind())
            && !self.is_furiten(seat)
            && self.evaluate_win(seat, tile, false, chankan).is_some()
    }

    fn can_tsumo(&self, seat: u8) -> bool {
        let p = &self.players[seat as usize];
        if p.drawn == NO_TILE || shanten(&self.counts(seat), p.meld_count) != -1 {
            return false;
        }
        self.evaluate_win(seat, tile_from_code(p.drawn), true, false)
            .is_some()
    }

    // ----------------------------------------------------------------
    // Legal actions
    // ----------------------------------------------------------------

    /// Everything `seat` may do right now. Empty if it isn't their decision.
    pub fn legal_actions(&self, seat: u8) -> ActionList {
        let mut out = ActionList::new();
        match self.phase {
            Phase::Turn { seat: s } if s == seat => self.turn_actions(seat, &mut out),
            Phase::AfterCall { seat: s, forbid } if s == seat => {
                for k in 0..NUM_KINDS as u8 {
                    if forbid.contains(&k) {
                        continue;
                    }
                    let (tiles, n) = self.distinct_tiles_of(seat, k);
                    for &t in &tiles[..n] {
                        out.push(Action::Discard(t));
                    }
                }
            }
            Phase::CallWindow {
                from,
                tile,
                pending,
                ..
            } if pending & bit(seat) != 0 => self.call_options(seat, from, tile, &mut out),
            Phase::ChankanWindow { pending, .. } if pending & bit(seat) != 0 => {
                out.push(Action::Ron);
                out.push(Action::Pass);
            }
            _ => {}
        }
        out
    }

    fn turn_actions(&self, seat: u8, out: &mut ActionList) {
        let p = &self.players[seat as usize];
        if self.can_tsumo(seat) {
            out.push(Action::Tsumo);
        }
        let first_turn = p.discard_len == 0 && self.no_calls_yet();
        if first_turn {
            let kinds = YAOCHUU.iter().filter(|&&k| p.hand[k as usize] > 0).count();
            if kinds >= 9 {
                out.push(Action::Kyuushu);
            }
        }
        let can_kan = self.tiles_left() > 0 && self.kan_count < 4;
        if can_kan {
            for k in 0..NUM_KINDS as u8 {
                if p.hand[k as usize] == 4 && (!p.in_riichi() || self.riichi_ankan_ok(seat, k)) {
                    out.push(Action::Ankan(k));
                }
            }
            if !p.in_riichi() {
                for m in p.melds() {
                    if m.kind() == MeldKind::Pon && p.hand[m.first() as usize] > 0 {
                        let (tiles, n) = self.distinct_tiles_of(seat, m.first());
                        out.push(Action::Kakan(tiles[n - 1]));
                    }
                }
            }
        }
        if p.in_riichi() {
            out.push(Action::Discard(tile_from_code(p.drawn)));
            return;
        }
        let riichi_ok =
            p.is_closed() && self.scores[seat as usize] >= 1000 && self.tiles_left() >= 4;
        for k in 0..NUM_KINDS as u8 {
            let (tiles, n) = self.distinct_tiles_of(seat, k);
            if n == 0 {
                continue;
            }
            let riichi_here = riichi_ok && {
                let mut c = self.counts(seat);
                c.0[k as usize] -= 1;
                shanten(&c, p.meld_count) == 0
            };
            for &t in &tiles[..n] {
                out.push(Action::Discard(t));
                if riichi_here {
                    out.push(Action::Riichi(t));
                }
            }
        }
    }

    /// Ankan during riichi: only with the tile just drawn, and only if the
    /// waits stay the same.
    fn riichi_ankan_ok(&self, seat: u8, kind: u8) -> bool {
        let p = &self.players[seat as usize];
        if p.drawn == NO_TILE || tile_from_code(p.drawn).kind() != kind {
            return false;
        }
        let mut before = self.counts(seat);
        before.0[kind as usize] -= 1;
        let mut after = self.counts(seat);
        after.0[kind as usize] = 0;
        waits(&before, p.meld_count) == waits(&after, p.meld_count + 1)
    }

    /// Ron, pon, daiminkan and chi options for `seat` on `from`'s discard.
    /// Pass is included whenever anything else is.
    fn call_options(&self, seat: u8, from: u8, tile: Tile, out: &mut ActionList) {
        if self.can_ron(seat, tile, false) {
            out.push(Action::Ron);
        }
        let p = &self.players[seat as usize];
        if !p.in_riichi() && self.tiles_left() > 0 {
            let k = tile.kind();
            let n = p.hand[k as usize];
            if n >= 2 && self.has_discard_after_call(seat, &[k, k], [k, NO_KIND]) {
                // Which two tiles to give up: plain+plain and/or red+plain.
                let red = is_five(k) && p.reds & red_bit(k) != 0;
                let plain = n - red as u8;
                if plain >= 2 {
                    out.push(Action::Pon([Tile::from_kind(k); 2]));
                }
                if red {
                    out.push(Action::Pon([red_tile(k), Tile::from_kind(k)]));
                }
            }
            if n == 3 && self.kan_count < 4 {
                out.push(Action::Daiminkan);
            }
            if seat == (from + 1) % 4 && k < 27 {
                self.chi_options(seat, k, out);
            }
        }
        if !out.is_empty() {
            out.push(Action::Pass);
        }
    }

    fn chi_options(&self, seat: u8, k: u8, out: &mut ActionList) {
        let n = k % 9;
        // (other two kinds, forbidden kinds after the call)
        let mut shapes: [(u8, u8, [u8; 2]); 3] = [(0, 0, [NO_KIND; 2]); 3];
        let mut len = 0;
        if n >= 2 {
            let other = if n >= 3 { k - 3 } else { NO_KIND };
            shapes[len] = (k - 2, k - 1, [k, other]);
            len += 1;
        }
        if (1..=7).contains(&n) {
            shapes[len] = (k - 1, k + 1, [k, NO_KIND]);
            len += 1;
        }
        if n <= 6 {
            let other = if n <= 5 { k + 3 } else { NO_KIND };
            shapes[len] = (k + 1, k + 2, [k, other]);
            len += 1;
        }
        for &(a, b, forbid) in &shapes[..len] {
            let (ta, na) = self.distinct_tiles_of(seat, a);
            let (tb, nb) = self.distinct_tiles_of(seat, b);
            if na == 0 || nb == 0 || !self.has_discard_after_call(seat, &[a, b], forbid) {
                continue;
            }
            for &x in &ta[..na] {
                for &y in &tb[..nb] {
                    out.push(Action::Chi([x, y]));
                }
            }
        }
    }

    /// After using `used` kinds from the hand for a call, is there at least
    /// one tile left that isn't forbidden by kuikae?
    fn has_discard_after_call(&self, seat: u8, used: &[u8], forbid: [u8; 2]) -> bool {
        let mut c = self.counts(seat);
        for &u in used {
            c.0[u as usize] -= 1;
        }
        (0..NUM_KINDS as u8).any(|k| c.0[k as usize] > 0 && !forbid.contains(&k))
    }

    // ----------------------------------------------------------------
    // Step
    // ----------------------------------------------------------------

    /// `seat` takes `action`, pushing what happened into `events`.
    pub fn step(
        &mut self,
        seat: u8,
        action: Action,
        events: &mut Vec<Event>,
    ) -> Result<(), StepError> {
        if matches!(self.phase, Phase::RoundEnd | Phase::GameEnd) {
            return Err(StepError::NotPlaying);
        }
        if seat >= 4 || self.waiting_on() & bit(seat) == 0 {
            return Err(StepError::NotYourTurn);
        }
        if !self.legal_actions(seat).contains(&action) {
            return Err(StepError::Illegal);
        }
        match self.phase {
            Phase::Turn { .. } => match action {
                Action::Discard(t) => self.discard(seat, t, false, events),
                Action::Riichi(t) => self.discard(seat, t, true, events),
                Action::Tsumo => self.tsumo(seat, events),
                Action::Kyuushu => self.abort(AbortKind::KyuushuKyuuhai, events),
                Action::Ankan(k) => self.ankan(seat, k, events),
                Action::Kakan(t) => self.kakan(seat, t, events),
                _ => unreachable!(),
            },
            Phase::AfterCall { .. } => match action {
                Action::Discard(t) => self.discard(seat, t, false, events),
                _ => unreachable!(),
            },
            Phase::CallWindow {
                from,
                tile,
                pending,
                mut ron,
                mut call_seat,
                mut call,
            } => {
                match action {
                    Action::Ron => ron |= bit(seat),
                    Action::Pass => {}
                    a => {
                        // Pon and daiminkan outrank chi; two pons on one
                        // tile can't happen.
                        let stronger = call_seat == NO_SEAT || !matches!(a, Action::Chi(_));
                        if stronger {
                            call_seat = seat;
                            call = a;
                        }
                    }
                }
                let pending = pending & !bit(seat);
                self.phase = Phase::CallWindow {
                    from,
                    tile,
                    pending,
                    ron,
                    call_seat,
                    call,
                };
                if pending == 0 {
                    self.resolve_call_window(events);
                }
            }
            Phase::ChankanWindow {
                from,
                tile,
                pending,
                mut ron,
                ankan,
            } => {
                if action == Action::Ron {
                    ron |= bit(seat);
                }
                let pending = pending & !bit(seat);
                self.phase = Phase::ChankanWindow {
                    from,
                    tile,
                    pending,
                    ron,
                    ankan,
                };
                if pending == 0 {
                    self.resolve_chankan_window(events);
                }
            }
            Phase::RoundEnd | Phase::GameEnd => unreachable!(),
        }
        Ok(())
    }

    fn discard(&mut self, seat: u8, t: Tile, riichi: bool, events: &mut Vec<Event>) {
        self.remove_from_hand(seat, t);
        let first_turn = self.players[seat as usize].discard_len == 0 && self.no_calls_yet();
        let p = &mut self.players[seat as usize];
        let tsumogiri = p.drawn == t.code();
        let i = p.discard_len as usize;
        debug_assert!(i < p.discards.len(), "discard cap exceeded");
        p.discards[i] = t.code()
            | if tsumogiri {
                discard_bits::TSUMOGIRI
            } else {
                0
            };
        p.discard_len += 1;
        p.drawn = NO_TILE;
        // Your own discard ends temporary furiten, your ippatsu chance and
        // the rinshan window.
        p.flags &= !(pf::TEMP_FURITEN | pf::IPPATSU | pf::RINSHAN);
        if riichi {
            p.flags |= pf::RIICHI_PENDING;
            if first_turn {
                p.flags |= pf::DOUBLE_RIICHI;
            }
            p.riichi_index = i as u8;
        }
        events.push(Event::Discard {
            seat,
            tile: t,
            tsumogiri,
            riichi,
        });
        self.reveal_pending_dora(events);
        self.open_call_window(seat, t, events);
    }

    fn open_call_window(&mut self, from: u8, tile: Tile, events: &mut Vec<Event>) {
        let mut pending = 0;
        for s in (0..4).filter(|&s| s != from) {
            let mut opts = ActionList::new();
            self.call_options(s, from, tile, &mut opts);
            if !opts.is_empty() {
                pending |= bit(s);
            }
        }
        self.phase = Phase::CallWindow {
            from,
            tile,
            pending,
            ron: 0,
            call_seat: NO_SEAT,
            call: Action::Pass,
        };
        if pending == 0 {
            self.resolve_call_window(events);
        }
    }

    /// Seats (other than `except`) whose shape `tile` completes but who
    /// didn't ron go into temporary (or riichi) furiten.
    fn mark_missed_wins(&mut self, from: u8, tile: Tile, ron: u8) {
        for s in (0..4).filter(|&s| s != from && ron & bit(s) == 0) {
            if self.completes(s, tile.kind()) {
                let p = &mut self.players[s as usize];
                p.flags |= pf::TEMP_FURITEN;
                if p.in_riichi() {
                    p.flags |= pf::RIICHI_FURITEN;
                }
            }
        }
    }

    fn resolve_call_window(&mut self, events: &mut Vec<Event>) {
        let Phase::CallWindow {
            from,
            tile,
            ron,
            call_seat,
            call,
            ..
        } = self.phase
        else {
            unreachable!()
        };
        let rons = ron.count_ones();
        if rons >= 3 && R::TRIPLE_RON_ABORTS {
            self.cancel_pending_riichi(from);
            self.abort(AbortKind::Sanchahou, events);
            return;
        }
        if rons > 0 {
            self.cancel_pending_riichi(from);
            let winners = self.seats_after(from, ron);
            let n = if R::DOUBLE_RON { winners.len() } else { 1 };
            self.ron_wins(&winners[..n], from, tile, false, events);
            return;
        }
        self.mark_missed_wins(from, tile, 0);
        self.accept_pending_riichi(from, events);
        if call_seat != NO_SEAT {
            self.apply_call(call_seat, from, tile, call, events);
            return;
        }
        self.after_discard_passed(from, events);
    }

    /// The seats in `mask`, in turn order starting after `from`.
    fn seats_after(&self, from: u8, mask: u8) -> Vec<u8> {
        (1..4)
            .map(|i| (from + i) % 4)
            .filter(|&s| mask & bit(s) != 0)
            .collect()
    }

    fn cancel_pending_riichi(&mut self, seat: u8) {
        let p = &mut self.players[seat as usize];
        if p.has(pf::RIICHI_PENDING) {
            p.flags &= !(pf::RIICHI_PENDING | pf::DOUBLE_RIICHI);
            p.riichi_index = NO_RIICHI;
        }
    }

    fn accept_pending_riichi(&mut self, seat: u8, events: &mut Vec<Event>) {
        let p = &mut self.players[seat as usize];
        if p.has(pf::RIICHI_PENDING) {
            p.flags &= !pf::RIICHI_PENDING;
            p.flags |= pf::RIICHI | pf::IPPATSU;
            self.scores[seat as usize] -= 1000;
            self.riichi_sticks += 1;
            events.push(Event::RiichiAccepted { seat });
        }
    }

    /// The discard passed with no ron and no call.
    fn after_discard_passed(&mut self, from: u8, events: &mut Vec<Event>) {
        // Suufon renda: the first four discards are the same wind, no calls.
        if self.no_calls_yet() && self.players.iter().all(|p| p.discard_len == 1) {
            let first = self.players[0].discards[0] & discard_bits::CODE_MASK;
            let k = tile_from_code(first).kind();
            if (EAST..EAST + 4).contains(&k)
                && self
                    .players
                    .iter()
                    .all(|p| p.discards[0] & discard_bits::CODE_MASK == first)
            {
                self.abort(AbortKind::SuufonRenda, events);
                return;
            }
        }
        if self.players.iter().all(|p| p.in_riichi()) {
            self.abort(AbortKind::SuuchaRiichi, events);
            return;
        }
        if self.kan_count == 4 && !self.players.iter().any(|p| self.kans_of(p) == 4) {
            self.abort(AbortKind::Suukaikan, events);
            return;
        }
        if self.tiles_left() == 0 {
            self.exhaustive_draw(events);
            return;
        }
        self.draw_live((from + 1) % 4, events);
    }

    fn kans_of(&self, p: &PlayerState) -> usize {
        p.melds().iter().filter(|m| m.kind().is_kan()).count()
    }

    fn break_ippatsu(&mut self) {
        for p in &mut self.players {
            p.flags &= !pf::IPPATSU;
        }
    }

    /// Where `from` sits relative to `seat`: 1 = next in turn order
    /// (right), 2 = across, 3 = previous (left).
    fn relative(from: u8, seat: u8) -> u8 {
        (from + 4 - seat) % 4
    }

    fn push_meld(&mut self, seat: u8, m: PackedMeld) {
        let p = &mut self.players[seat as usize];
        p.melds[p.meld_count as usize] = m;
        p.meld_count += 1;
    }

    fn mark_last_discard_called(&mut self, from: u8) {
        let p = &mut self.players[from as usize];
        let i = p.discard_len as usize - 1;
        p.discards[i] |= discard_bits::CALLED;
    }

    fn apply_call(
        &mut self,
        seat: u8,
        from: u8,
        tile: Tile,
        action: Action,
        events: &mut Vec<Event>,
    ) {
        self.break_ippatsu();
        self.mark_last_discard_called(from);
        let k = tile.kind();
        let rel = Self::relative(from, seat);
        events.push(Event::Call {
            seat,
            from,
            tile,
            action,
        });
        match action {
            Action::Chi([a, b]) => {
                self.remove_from_hand(seat, a);
                self.remove_from_hand(seat, b);
                let first = k.min(a.kind()).min(b.kind());
                let red = tile.is_red() || a.is_red() || b.is_red();
                self.push_meld(seat, PackedMeld::pack(MeldKind::Chi, first, rel, red));
                let mid = first + 1;
                let forbid = if k == mid {
                    [k, NO_KIND]
                } else if k == first {
                    [k, if k % 9 <= 5 { k + 3 } else { NO_KIND }]
                } else {
                    [k, if k % 9 >= 3 { k - 3 } else { NO_KIND }]
                };
                self.phase = Phase::AfterCall { seat, forbid };
            }
            Action::Pon([a, b]) => {
                self.remove_from_hand(seat, a);
                self.remove_from_hand(seat, b);
                let red = tile.is_red() || a.is_red() || b.is_red();
                self.push_meld(seat, PackedMeld::pack(MeldKind::Pon, k, rel, red));
                self.check_pao(seat, from, k);
                self.phase = Phase::AfterCall {
                    seat,
                    forbid: [k, NO_KIND],
                };
            }
            Action::Daiminkan => {
                let red = tile.is_red()
                    || (is_five(k) && self.players[seat as usize].reds & red_bit(k) != 0);
                let p = &mut self.players[seat as usize];
                p.hand[k as usize] = 0;
                if is_five(k) {
                    p.reds &= !red_bit(k);
                }
                self.push_meld(seat, PackedMeld::pack(MeldKind::Daiminkan, k, rel, red));
                self.check_pao(seat, from, k);
                self.kan_count += 1;
                // Open kan: the new indicator flips at the next discard.
                self.pending_dora += 1;
                self.draw_rinshan(seat, events);
            }
            _ => unreachable!(),
        }
    }

    /// Daisangen / daisuushi liability: the discard that completed the
    /// third dragon set or the fourth wind set.
    fn check_pao(&mut self, seat: u8, from: u8, kind: u8) {
        let sets_of = |range: std::ops::RangeInclusive<u8>| {
            self.players[seat as usize]
                .melds()
                .iter()
                .filter(|m| m.kind() != MeldKind::Chi && range.contains(&m.first()))
                .count()
        };
        if (31..=33).contains(&kind) && sets_of(31..=33) == 3 {
            self.pao[seat as usize] = from;
        }
        if (27..=30).contains(&kind) && sets_of(27..=30) == 4 {
            self.pao[seat as usize] = from;
        }
    }

    fn ankan(&mut self, seat: u8, k: u8, events: &mut Vec<Event>) {
        self.break_ippatsu();
        self.reveal_pending_dora(events);
        let p = &mut self.players[seat as usize];
        let red = is_five(k) && p.reds & red_bit(k) != 0;
        p.hand[k as usize] = 0;
        if is_five(k) {
            p.reds &= !red_bit(k);
        }
        p.drawn = NO_TILE;
        self.push_meld(seat, PackedMeld::pack(MeldKind::Ankan, k, 0, red));
        events.push(Event::Kan {
            seat,
            action: Action::Ankan(k),
        });
        // Only kokushi may rob a closed kan.
        let tile = if red { red_tile(k) } else { Tile::from_kind(k) };
        let mut pending = 0;
        for s in (0..4).filter(|&s| s != seat) {
            let mut c = self.counts(s);
            if self.meld_count(s) == 0 && c.0[k as usize] < 4 {
                c.0[k as usize] += 1;
                if usagi_core::shanten::kokushi(&c) == -1 && self.can_ron(s, tile, true) {
                    pending |= bit(s);
                }
            }
        }
        self.phase = Phase::ChankanWindow {
            from: seat,
            tile,
            pending,
            ron: 0,
            ankan: true,
        };
        if pending == 0 {
            self.resolve_chankan_window(events);
        }
    }

    fn kakan(&mut self, seat: u8, t: Tile, events: &mut Vec<Event>) {
        self.reveal_pending_dora(events);
        self.remove_from_hand(seat, t);
        let k = t.kind();
        let p = &mut self.players[seat as usize];
        p.drawn = NO_TILE;
        let i = p
            .melds()
            .iter()
            .position(|m| m.kind() == MeldKind::Pon && m.first() == k)
            .expect("kakan needs a pon");
        let red = p.melds[i].has_red() || t.is_red();
        p.melds[i] = p.melds[i].with_kind(MeldKind::Kakan, red);
        events.push(Event::Kan {
            seat,
            action: Action::Kakan(t),
        });
        let mut pending = 0;
        for s in (0..4).filter(|&s| s != seat) {
            if self.can_ron(s, t, true) {
                pending |= bit(s);
            }
        }
        self.phase = Phase::ChankanWindow {
            from: seat,
            tile: t,
            pending,
            ron: 0,
            ankan: false,
        };
        if pending == 0 {
            self.resolve_chankan_window(events);
        }
    }

    fn resolve_chankan_window(&mut self, events: &mut Vec<Event>) {
        let Phase::ChankanWindow {
            from,
            tile,
            ron,
            ankan,
            ..
        } = self.phase
        else {
            unreachable!()
        };
        if ron.count_ones() >= 3 && R::TRIPLE_RON_ABORTS {
            self.abort(AbortKind::Sanchahou, events);
            return;
        }
        if ron != 0 {
            let winners = self.seats_after(from, ron);
            let n = if R::DOUBLE_RON { winners.len() } else { 1 };
            self.ron_wins(&winners[..n], from, tile, true, events);
            return;
        }
        self.mark_missed_wins(from, tile, 0);
        self.break_ippatsu();
        self.kan_count += 1;
        if ankan {
            self.reveal_dora(events);
        } else {
            self.pending_dora += 1;
        }
        self.draw_rinshan(from, events);
    }

    fn tsumo(&mut self, seat: u8, events: &mut Vec<Event>) {
        let tile = tile_from_code(self.players[seat as usize].drawn);
        let result = self
            .evaluate_win(seat, tile, true, false)
            .expect("legal tsumo scores");
        let dealer = self.dealer();
        let pay = payment(result.base, seat == dealer, true);
        let mut deltas = score_deltas(
            pay,
            seat as usize,
            None,
            dealer as usize,
            self.round.honba,
            self.riichi_sticks,
        );
        if let Some(liable) = self.pao_for(seat, &result) {
            // The liable player pays the whole tsumo.
            let total: i32 = deltas.iter().filter(|&&d| d < 0).sum();
            for (s, d) in deltas.iter_mut().enumerate() {
                if *d < 0 {
                    *d = 0;
                }
                if s as u8 == liable {
                    *d = total;
                }
            }
        }
        events.push(Event::Win {
            seat,
            from: NO_SEAT,
            tile,
            han: result.han,
            fu: result.fu,
            yakuman: result.yakuman,
            points: pay.total(),
        });
        self.riichi_sticks = 0;
        self.finish_round(deltas, seat == dealer, HandEnd::Win, events);
    }

    fn pao_for(&self, seat: u8, result: &WinResult) -> Option<u8> {
        let liable = self.pao[seat as usize];
        let pao_yaku = result
            .yaku
            .iter()
            .any(|y| matches!(y, Yaku::Daisangen | Yaku::Daisuushi));
        (liable != NO_SEAT && pao_yaku).then_some(liable)
    }

    fn ron_wins(
        &mut self,
        winners: &[u8],
        from: u8,
        tile: Tile,
        chankan: bool,
        events: &mut Vec<Event>,
    ) {
        let dealer = self.dealer();
        let mut total = [0i32; 4];
        for (i, &seat) in winners.iter().enumerate() {
            let result = self
                .evaluate_win(seat, tile, false, chankan)
                .expect("legal ron scores");
            let pay = payment(result.base, seat == dealer, false);
            // Honba and riichi sticks go to the first winner in turn order.
            let (honba, sticks) = if i == 0 {
                (self.round.honba, self.riichi_sticks)
            } else {
                (0, 0)
            };
            let mut deltas = score_deltas(
                pay,
                seat as usize,
                Some(from as usize),
                dealer as usize,
                honba,
                sticks,
            );
            if let Some(liable) = self.pao_for(seat, &result) {
                if liable != from {
                    // Liability on ron: the liable player and the discarder
                    // split the hand's value (honba stay with the discarder).
                    let Payment::Ron(points) = pay else {
                        unreachable!()
                    };
                    let half = (points / 2) as i32;
                    deltas[from as usize] += half;
                    deltas[liable as usize] -= half;
                }
            }
            for s in 0..4 {
                total[s] += deltas[s];
            }
            events.push(Event::Win {
                seat,
                from,
                tile,
                han: result.han,
                fu: result.fu,
                yakuman: result.yakuman,
                points: pay.total(),
            });
        }
        self.riichi_sticks = 0;
        let renchan = winners.contains(&dealer);
        self.finish_round(total, renchan, HandEnd::Win, events);
    }

    // ----------------------------------------------------------------
    // Draws and round end
    // ----------------------------------------------------------------

    fn abort(&mut self, kind: AbortKind, events: &mut Vec<Event>) {
        events.push(Event::AbortiveDraw { kind });
        // Abortive draws repeat the hand: same dealer, honba + 1, sticks stay.
        self.finish_round([0; 4], true, HandEnd::Abort, events);
    }

    fn is_tenpai(&self, seat: u8) -> bool {
        // A hand whose only wait is a tile it holds all four of is noten.
        !waits(&self.counts(seat), self.meld_count(seat)).is_empty()
    }

    fn exhaustive_draw(&mut self, events: &mut Vec<Event>) {
        let dealer = self.dealer();
        let mut tenpai = 0u8;
        let mut nagashi = 0u8;
        for s in 0..4u8 {
            if self.is_tenpai(s) {
                tenpai |= bit(s);
            }
            let p = &self.players[s as usize];
            let all_terminal = p.discards().iter().all(|&d| {
                d & discard_bits::CALLED == 0
                    && tile_from_code(d & discard_bits::CODE_MASK).is_terminal_or_honor()
            });
            if p.discard_len > 0 && all_terminal {
                nagashi |= bit(s);
            }
        }
        let mut deltas = [0i32; 4];
        if nagashi != 0 {
            // Each nagashi mangan is paid like a mangan tsumo.
            for s in (0..4)
                .map(|i| (dealer + i) % 4)
                .filter(|&s| nagashi & bit(s) != 0)
            {
                let pay = payment(2000, s == dealer, true);
                let d = score_deltas(pay, s as usize, None, dealer as usize, 0, 0);
                for i in 0..4 {
                    deltas[i] += d[i];
                }
            }
        } else {
            let n = tenpai.count_ones() as i32;
            if (1..=3).contains(&n) {
                for (s, d) in deltas.iter_mut().enumerate() {
                    *d = if tenpai & bit(s as u8) != 0 {
                        3000 / n
                    } else {
                        -3000 / (4 - n)
                    };
                }
            }
        }
        events.push(Event::ExhaustiveDraw { tenpai, nagashi });
        self.finish_round(deltas, tenpai & bit(dealer) != 0, HandEnd::Draw, events);
    }

    /// Applies the hand's score changes, moves the dealer and honba, and
    /// decides whether the game is over.
    fn finish_round(
        &mut self,
        deltas: [i32; 4],
        renchan: bool,
        end: HandEnd,
        events: &mut Vec<Event>,
    ) {
        for (score, d) in self.scores.iter_mut().zip(deltas) {
            *score += d;
        }
        events.push(Event::RoundEnded { deltas });
        let index = self.round.index;
        let dealer = self.dealer() as usize;

        let over = if R::TOBI && self.scores.iter().any(|&s| s < 0) {
            true
        } else if index < 7 || end == HandEnd::Abort {
            false
        } else if renchan {
            // All-last dealer stop: a repeating dealer who is first with
            // the target score ends the game.
            self.ranks()[dealer] == 0 && self.scores[dealer] >= R::TARGET_POINTS
        } else {
            // After South 4 the game ends once anyone has the target score
            // (otherwise it goes into the West round); West 4 is final.
            self.scores.iter().any(|&s| s >= R::TARGET_POINTS) || index >= 11
        };

        if renchan {
            self.round.honba += 1;
        } else {
            self.round.index += 1;
            self.round.honba = if end == HandEnd::Win {
                0
            } else {
                self.round.honba + 1
            };
        }
        if over {
            self.end_game(events);
        } else {
            self.phase = Phase::RoundEnd;
        }
    }

    fn end_game(&mut self, events: &mut Vec<Event>) {
        if self.riichi_sticks > 0 {
            let ranks = self.ranks();
            let top = (0..4).find(|&s| ranks[s] == 0).unwrap();
            self.scores[top] += 1000 * self.riichi_sticks as i32;
            self.riichi_sticks = 0;
        }
        self.phase = Phase::GameEnd;
        events.push(Event::GameEnded {
            scores: self.scores,
        });
    }
}

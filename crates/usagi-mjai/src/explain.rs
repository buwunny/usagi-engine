//! Why a bot made a move: a structured record, built only when asked.
//!
//! Every bot that plays on usagi.club can implement [`Explain`]. The record
//! has the same parts for every bot, so the site can show any of them:
//!
//! - **hand progress**: shanten, tile acceptance and waits after the move;
//! - **opponents**: each opponent's chance of being ready (tenpai);
//! - **danger**: the chance each tile in hand deals in;
//! - **win chance** and **placement**;
//! - the **candidates** it weighed, best first, and the **reasons** that
//!   decided it.
//!
//! The rule-based bots fill these from hand-written formulas. A learned
//! bot (bunny bot) fills the same fields from its own extra outputs, so
//! the site doesn't change when it arrives.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use usagi_core::Tile;

use crate::event::{Event, Pai};
use crate::table::Bot;

/// A bot that can say why it made its last move.
pub trait Explain {
    /// Explains the most recent decision (`None` before the first one, or
    /// when the bot can't explain). Called only when someone asks, so the
    /// work happens here and not while playing.
    fn explain(&self) -> Option<Explanation>;
}

/// What usagi.club seats: plays over Mjai and explains on request.
pub trait SiteBot: Bot + Explain + Send {
    /// A short name for the bot, like `"hard"`.
    fn label(&self) -> String;
}

/// Which kind of decision it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionKind {
    /// After our draw: discard, riichi, tsumo, kan or kyuushu.
    Turn,
    /// Someone discarded (or added to a kan): ron, call or pass.
    Call,
    /// The discard right after our chi or pon.
    AfterCall,
}

/// The overall plan behind the move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stance {
    /// Building the hand; nobody looks threatening.
    Build,
    /// Someone looks ready, and we keep going anyway.
    Push,
    /// Someone looks ready; we keep the hand alive but avoid danger.
    Balance,
    /// Giving up on winning to avoid dealing in.
    Fold,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandProgress {
    /// Shanten after the move (-1 for a win, 0 for tenpai).
    pub shanten: i8,
    /// Unseen tiles that would lower the shanten.
    pub ukeire: u32,
    /// The winning tiles when tenpai.
    pub waits: Vec<Pai>,
    /// Rough points the hand is worth if it wins.
    pub value: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OpponentRead {
    pub seat: u8,
    /// Chance this seat is tenpai (1 in riichi).
    pub tenpai: f32,
    pub riichi: bool,
    pub melds: u8,
}

/// Why a tile is as safe as it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SafetyReason {
    /// The threatening players can't ron it (their own discard, or passed
    /// since their riichi).
    Genbutsu,
    /// The threats' discards rule out the two-sided waits on it.
    Suji,
    /// Every copy of a neighbour is visible, so no two-sided wait uses it.
    NoChance,
    /// An honor with several copies already visible.
    Honor,
    Terminal,
    /// Nothing makes it safer.
    Plain,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TileDanger {
    pub pai: Pai,
    /// Chance discarding it deals in, from what we can see.
    pub deal_in: f32,
    pub reason: SafetyReason,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    /// Our rank right now (1..=4).
    pub rank: u8,
    pub scores: [i32; 4],
    pub all_last: bool,
    /// Predicted final rank chances, when the bot predicts them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predicted: Option<[f32; 4]>,
}

/// One option the bot weighed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    pub action: Event,
    /// The bot's score for it; higher is better. Only comparable within
    /// one decision.
    pub score: f32,
    pub shanten: i8,
    pub ukeire: u32,
    pub deal_in: f32,
    pub win_chance: f32,
    pub value: u32,
}

/// The facts that decided the move, for turning into words.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Reason {
    /// The hand wins here for about this many points.
    Win { points: u32 },
    /// Kept the discard that leaves the most tiles to improve the hand.
    MostAcceptance { ukeire: u32 },
    /// The discard reaches tenpai on these tiles.
    Tenpai { waits: Vec<Pai> },
    /// Kept a better wait for after the next draw.
    BetterShape,
    /// Kept a dora (or red five).
    KeepDora,
    /// Kept tiles that give the hand a yaku.
    KeepYaku,
    /// Gave up on the hand because these seats look ready.
    Fold { against: Vec<u8> },
    /// Pushed because the hand is worth the risk.
    Push { win_chance: f32, value: u32 },
    /// The chosen tile is safe for this reason.
    Safe { reason: SafetyReason },
    /// Declared riichi.
    Riichi,
    /// Stayed quiet: the hand already has a yaku and enough value.
    Dama { value: u32 },
    /// Called to make a value triplet (dragon or own/round wind).
    YakuhaiCall,
    /// Called for all simples.
    TanyaoCall,
    /// Didn't call: it wouldn't help or would leave no yaku.
    NoCall,
    /// Took a closed kan without hurting the hand.
    Kan,
    /// Nine different terminals and honors: called off the hand.
    Kyuushu,
    /// Playing safe or fast for placement in the last hand.
    Placement { rank: u8 },
    /// A deliberately imperfect move (lower levels).
    Slip,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Explanation {
    /// The bot that decided, like `"rule bot (hard)"`.
    pub bot: String,
    pub seat: u8,
    pub kind: DecisionKind,
    pub chosen: Event,
    /// With a `reach`: the tile it discards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub riichi_discard: Option<Pai>,
    pub stance: Stance,
    pub hand: HandProgress,
    pub opponents: Vec<OpponentRead>,
    /// Every distinct tile kind in hand, safest first.
    pub danger: Vec<TileDanger>,
    pub win_chance: f32,
    pub placement: Placement,
    /// Best first.
    pub candidates: Vec<Candidate>,
    pub reasons: Vec<Reason>,
}

impl Explanation {
    /// A plain-English sentence or two, for logs and as a starting point
    /// for the site's wording.
    pub fn summary(&self) -> String {
        let mut s = String::new();
        let mut act = describe(&self.chosen);
        if let Some(p) = self.riichi_discard {
            act = format!("{act}, discarding {}", name(p));
        }
        let _ = write!(s, "{act}.");
        for r in &self.reasons {
            let line = match r {
                Reason::Win { points } => format!("It wins about {points} points."),
                Reason::MostAcceptance { ukeire } => {
                    format!("It keeps the most tiles that improve the hand ({ukeire} left).")
                }
                Reason::Tenpai { waits } => {
                    format!("The hand is ready, waiting on {}.", list(waits))
                }
                Reason::BetterShape => "It keeps a better wait for later.".into(),
                Reason::KeepDora => "It holds on to dora.".into(),
                Reason::KeepYaku => "It keeps the tiles that give the hand a yaku.".into(),
                Reason::Fold { against } => format!(
                    "{} looks ready, so it gives up on the hand.",
                    seats(against)
                ),
                Reason::Push { win_chance, value } => format!(
                    "The hand is worth about {value} points with a {:.0}% chance to win, so it pushes.",
                    win_chance * 100.0
                ),
                Reason::Safe { reason } => format!(
                    "The tile is {}.",
                    match reason {
                        SafetyReason::Genbutsu => "completely safe (genbutsu)",
                        SafetyReason::Suji => "fairly safe (suji)",
                        SafetyReason::NoChance => "fairly safe (no-chance)",
                        SafetyReason::Honor => "a fairly safe honor",
                        SafetyReason::Terminal => "a terminal, somewhat safer",
                        SafetyReason::Plain => "not especially safe",
                    }
                ),
                Reason::Riichi => "It declares riichi for value.".into(),
                Reason::Dama { value } => {
                    format!("It stays quiet: the hand already has a yaku and is worth {value}.")
                }
                Reason::YakuhaiCall => "The triplet is a yaku.".into(),
                Reason::TanyaoCall => "It goes for all simples.".into(),
                Reason::NoCall => "Calling wouldn't help the hand.".into(),
                Reason::Kan => "The kan doesn't hurt the hand.".into(),
                Reason::Kyuushu => "Nine terminals and honors: it calls off the hand.".into(),
                Reason::Placement { rank } => {
                    format!("It plays for placement from rank {rank}.")
                }
                Reason::Slip => "(A beginner's choice, not the best one.)".into(),
            };
            s.push(' ');
            s.push_str(&line);
        }
        s
    }
}

fn describe(e: &Event) -> String {
    match e {
        Event::Dahai { pai, .. } => format!("Discard {}", name(*pai)),
        Event::Reach { .. } => "Riichi".into(),
        Event::Hora { actor, target, .. } if actor == target => "Tsumo".into(),
        Event::Hora { .. } => "Ron".into(),
        Event::Chi { pai, .. } => format!("Chi on {}", name(*pai)),
        Event::Pon { pai, .. } => format!("Pon on {}", name(*pai)),
        Event::Ankan { consumed, .. } => format!(
            "Closed kan of {}",
            consumed.first().map_or("?".into(), |&p| name(p))
        ),
        Event::Ryukyoku { .. } => "Kyuushu kyuuhai".into(),
        Event::None => "Pass".into(),
        other => format!("{other:?}"),
    }
}

fn list(ps: &[Pai]) -> String {
    ps.iter().map(|&p| name(p)).collect::<Vec<_>>().join(", ")
}

/// A tile as a person would say it: `5m`, `red 5p`, `West`, `White`.
fn name(p: Pai) -> String {
    const HONORS: [&str; 7] = ["East", "South", "West", "North", "White", "Green", "Red"];
    match p.tile() {
        Some(t) if t.kind() >= 27 => HONORS[(t.kind() - 27) as usize].to_string(),
        Some(t) if t.is_red() => format!("red {}", Pai::from(Tile::from_kind(t.kind()))),
        _ => p.to_string(),
    }
}

fn seats(s: &[u8]) -> String {
    let names: Vec<String> = s.iter().map(|x| format!("seat {x}")).collect();
    names.join(" and ")
}

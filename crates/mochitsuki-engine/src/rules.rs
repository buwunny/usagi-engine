//! Rule variants.
//!
//! Riichi has many house rules. The engine reads every rule choice from a
//! `Rules` type, so another ruleset is "another type that implements
//! `Rules`" instead of `if`s scattered everywhere. v1 only has
//! [`TenhouRules`].
//!
//! Because the rules are *associated constants* on a type (not fields on a
//! value), they cost zero bytes in `GameState` and the compiler folds
//! `if R::DOUBLE_RON { ... }` away completely.

/// The rule knobs the engine consults.
///
/// `Copy + Default + 'static` lets `GameState<R>` stay `Copy` and lets the
/// engine make an `R` out of thin air when it needs one.
pub trait Rules: Copy + Default + Send + Sync + 'static {
    /// Points each player starts with.
    const STARTING_POINTS: i32;
    /// Points needed to end the game after South 4 ("oorasu"). If nobody
    /// reaches it, the game goes into the West round.
    const TARGET_POINTS: i32;
    /// Number of red fives in the set (0, 3 or 4).
    const RED_FIVES: u8;
    /// Open tanyao (*kuitan*) allowed.
    const KUITAN: bool;
    /// Two players may win on the same discard.
    const DOUBLE_RON: bool;
    /// Three players ronning the same discard aborts the hand instead.
    const TRIPLE_RON_ABORTS: bool;
    /// The game ends as soon as anyone's score goes below zero (*tobi*).
    const TOBI: bool;
}

/// Tenhou's four-player ranked rules.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct TenhouRules;

impl Rules for TenhouRules {
    // See docs/reference/tenhou-rules.md.
    const STARTING_POINTS: i32 = 25_000;
    const TARGET_POINTS: i32 = 30_000;
    const RED_FIVES: u8 = 3;
    const KUITAN: bool = true;
    const DOUBLE_RON: bool = true;
    const TRIPLE_RON_ABORTS: bool = true;
    const TOBI: bool = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    // cargo test -p mochitsuki-engine rules::

    #[test]
    fn tenhou_values() {
        assert_eq!(TenhouRules::STARTING_POINTS, 25_000);
        assert_eq!(TenhouRules::TARGET_POINTS, 30_000);
        assert_eq!(TenhouRules::RED_FIVES, 3);
        // kuitan, double ron, triple ron aborts, tobi
        let flags = [
            TenhouRules::KUITAN,
            TenhouRules::DOUBLE_RON,
            TenhouRules::TRIPLE_RON_ABORTS,
            TenhouRules::TOBI,
        ];
        assert_eq!(flags, [true; 4]);
    }
}

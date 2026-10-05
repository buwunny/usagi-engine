//! Actions a player can take.

use mochitsuki_core::Tile;

/// One decision by one player.
///
/// The drawn tile is not an action: drawing is automatic. Every decision
/// a player actually makes is one of these.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    /// Discard a tile (on your own turn, or right after a chi/pon).
    Discard(Tile),
    /// Declare riichi and discard this tile sideways.
    Riichi(Tile),
    /// Win on your own draw.
    Tsumo,
    /// Win on another player's discard (or their kakan tile: chankan).
    Ron,
    /// Call the discard from your left to make a sequence, using these two
    /// tiles from your hand.
    Chi([Tile; 2]),
    /// Call a discard to make a triplet, using these two tiles from your hand.
    Pon([Tile; 2]),
    /// Call a discard to make an open kan (you hold the other three).
    Daiminkan,
    /// Declare a closed kan of this kind on your turn.
    Ankan(u8),
    /// Add this tile to your existing pon of the same kind, on your turn.
    Kakan(Tile),
    /// Abort the hand with nine or more different terminals/honors on your
    /// first draw (*kyuushu kyuuhai*).
    Kyuushu,
    /// Decline every call option.
    Pass,
}

/// Upper bound on legal actions at once. A turn can offer up to 14
/// discards, 14 riichi discards, tsumo, a few kans and kyuushu; a call
/// window offers ron, a few pon/chi shapes (red five variants count
/// separately), daiminkan and pass.
pub const MAX_ACTIONS: usize = 48;

/// A fixed-capacity list of actions. No heap allocation, so it's cheap to
/// create millions of them, and it's `Copy`.
#[derive(Clone, Copy, Debug)]
pub struct ActionList {
    items: [Action; MAX_ACTIONS],
    len: u8,
}

impl ActionList {
    pub fn new() -> ActionList {
        ActionList {
            items: [Action::Pass; MAX_ACTIONS],
            len: 0,
        }
    }

    /// Appends an action. Panics if full (that would be an engine bug).
    pub fn push(&mut self, action: Action) {
        assert!((self.len as usize) < MAX_ACTIONS, "ActionList full");
        self.items[self.len as usize] = action;
        self.len += 1;
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn contains(&self, action: &Action) -> bool {
        self.as_slice().contains(action)
    }

    /// The actions as a slice. `ActionList` also derefs to it, so `iter()`
    /// and indexing work on the list directly.
    pub fn as_slice(&self) -> &[Action] {
        &self.items[..self.len as usize]
    }
}

impl std::ops::Deref for ActionList {
    type Target = [Action];

    fn deref(&self) -> &[Action] {
        self.as_slice()
    }
}

impl Default for ActionList {
    fn default() -> Self {
        ActionList::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // cargo test -p mochitsuki-engine action::

    #[test]
    fn push_and_read() {
        let mut list = ActionList::new();
        assert!(list.is_empty());
        list.push(Action::Tsumo);
        list.push(Action::Discard(Tile::from_kind(3)));
        assert_eq!(list.len(), 2);
        assert!(list.contains(&Action::Tsumo));
        assert!(!list.contains(&Action::Ron));
        assert_eq!(list.as_slice()[1], Action::Discard(Tile::from_kind(3)));
    }

    #[test]
    fn action_is_small() {
        // An Action should stay a few bytes; they get copied a lot.
        assert!(std::mem::size_of::<Action>() <= 4);
    }
}

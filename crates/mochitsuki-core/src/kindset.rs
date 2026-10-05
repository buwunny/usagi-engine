//! A set of tile kinds packed into one integer.
//!
//! Waits ("which tiles complete my hand?") and tile acceptance are sets of
//! kinds. With only 34 kinds, a set fits in the bits of one `u64`: bit `k`
//! is set when kind `k` is in the set. Union is `|`, intersection is `&`,
//! and size is `count_ones()`, all single CPU instructions.

/// A set of tile kinds (0..=33). Bit `k` of the `u64` = "kind `k` is in the set".
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KindSet(pub u64);

impl KindSet {
    pub const EMPTY: KindSet = KindSet(0);

    /// Adds `kind` to the set.
    pub fn insert(&mut self, kind: u8) {
        self.0 |= 1u64 << kind;
    }

    /// Removes `kind` from the set (no-op if absent).
    pub fn remove(&mut self, kind: u8) {
        self.0 &= !(1u64 << kind);
    }

    pub fn contains(self, kind: u8) -> bool {
        self.0 >> kind & 1 == 1
    }

    pub fn len(self) -> u32 {
        self.0.count_ones()
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn union(self, other: KindSet) -> KindSet {
        KindSet(self.0 | other.0)
    }

    /// The kinds in the set, in increasing order.
    pub fn iter(self) -> impl Iterator<Item = u8> {
        // Two ways: filter 0..34 with `contains`, or the bit trick: take
        // `bits.trailing_zeros()` as the next kind, then clear the lowest
        // set bit with `bits &= bits - 1`. `std::iter::from_fn` turns a
        // closure returning `Option<u8>` into an iterator.
        let mut bits = self.0;
        std::iter::from_fn(move || {
            if bits == 0 {
                None
            } else {
                let k = bits.trailing_zeros() as u8;
                bits &= bits - 1;
                Some(k)
            }
        })
    }
}

/// Builds a set from kinds: `KindSet::from_iter([0, 3, 27])`, or
/// `.collect::<KindSet>()` at the end of an iterator chain.
impl FromIterator<u8> for KindSet {
    fn from_iter<I: IntoIterator<Item = u8>>(iter: I) -> KindSet {
        let mut s = KindSet::EMPTY;
        for k in iter {
            s.insert(k);
        }
        s
    }
}

/// Prints like `{1m, 4m, 7m}`, which makes failed `assert_eq!`s readable.
impl std::fmt::Debug for KindSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        struct D(crate::tile::Tile);
        impl std::fmt::Debug for D {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
        f.debug_set()
            .entries(self.iter().map(|k| D(crate::tile::Tile::from_kind(k))))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // cargo test -p mochitsuki-core kindset::

    #[test]
    fn basic_ops() {
        let mut s = KindSet::EMPTY;
        assert!(s.is_empty());
        s.insert(0);
        s.insert(33);
        s.insert(0);
        assert!(s.contains(0) && s.contains(33) && !s.contains(1));
        assert_eq!(s.len(), 2);
        s.remove(0);
        s.remove(5);
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![33]);
    }

    #[test]
    fn collect_union_iter() {
        let a: KindSet = [1, 4, 7].into_iter().collect();
        let b = KindSet::from_iter([7, 30]);
        assert_eq!(a.union(b).iter().collect::<Vec<_>>(), vec![1, 4, 7, 30]);
        assert_eq!(format!("{:?}", a), "{2m, 5m, 8m}");
    }
}

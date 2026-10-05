//! A small, fast, seedable random number generator.
//!
//! The engine needs randomness only to shuffle the wall, and it needs it to
//! be *reproducible*: the same seed must give the same wall on every
//! machine, forever, so a game can be replayed from its seed. That rules
//! out `rand`'s default generator, whose algorithm may change between
//! versions.
//!
//! **SplitMix64** expands the seed, then **xoshiro256\*\*** generates.
//! Both are public domain.
//!
//! (Replaying real Tenhou games doesn't use this at all: those walls come
//! from the logs. See M6.)

/// xoshiro256** state.
#[derive(Clone, Debug)]
pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// Seeds the generator. Use SplitMix64 on `seed` to fill the four
    /// state words (xoshiro must never have an all-zero state).
    pub fn new(seed: u64) -> Rng {
        let mut x = seed;
        let mut split = || {
            x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Rng {
            s: [split(), split(), split(), split()],
        }
    }

    /// The next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        let s = &mut self.s;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// A uniform integer in `0..n`. `n` must be > 0.
    ///
    /// `next_u64() % n` is slightly biased toward small numbers. For a
    /// 136-tile shuffle the bias is about 1 in 10^17, which is fine, but
    /// look up "Lemire's method" if you want it exact.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0);
        // Lemire's multiply-shift with rejection: exact.
        let threshold = n.wrapping_neg() % n;
        loop {
            let m = (self.next_u64() as u128) * (n as u128);
            if (m as u64) >= threshold {
                return (m >> 64) as u64;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // cargo test -p mochitsuki-engine rng::

    #[test]
    fn same_seed_same_numbers() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_differ() {
        assert_ne!(Rng::new(1).next_u64(), Rng::new(2).next_u64());
    }

    #[test]
    fn known_first_output() {
        // xoshiro256** seeded through SplitMix64 from 0. If you implement
        // both exactly as published, this is the first output.
        assert_eq!(Rng::new(0).next_u64(), 0x99EC_5F36_CB75_F2B4);
    }

    #[test]
    fn below_stays_in_range() {
        let mut r = Rng::new(7);
        let mut seen = [false; 6];
        for _ in 0..1000 {
            let x = r.below(6);
            assert!(x < 6);
            seen[x as usize] = true;
        }
        assert!(seen.iter().all(|&s| s), "every value should appear");
    }
}

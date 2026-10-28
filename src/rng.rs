//! A small seeded random number generator.
//!
//! The sequence is part of the program's contract: the same seed must build the
//! same fight across versions, so this does not depend on an external crate.

/// A xorshift64 generator, seeded through splitmix64.
#[derive(Debug, Clone)]
pub struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    /// Creates a generator. Every seed, including zero, gives a usable sequence.
    pub fn new(seed: u64) -> Self {
        // splitmix64 spreads small seeds over the whole state.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        // Xorshift never leaves the all-zero state.
        let state = if z == 0 { 0x9E37_79B9_7F4A_7C15 } else { z };
        Self { state }
    }

    /// Returns the next raw value.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Returns a value in `low..=high`.
    pub fn range(&mut self, low: u32, high: u32) -> u32 {
        debug_assert!(low <= high);
        let span = u64::from(high - low) + 1;
        // The modulo bias is negligible for the small spans used here.
        low + (self.next_u64() % span) as u32
    }

    /// Picks one element of a non-empty slice.
    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.range(0, items.len() as u32 - 1) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = XorShift64::new(7);
        let mut b = XorShift64::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_differ() {
        assert_ne!(XorShift64::new(1).next_u64(), XorShift64::new(2).next_u64());
    }

    #[test]
    fn zero_seed_is_not_stuck() {
        let mut rng = XorShift64::new(0);
        let first = rng.next_u64();
        assert_ne!(first, 0);
        assert_ne!(first, rng.next_u64());
    }

    #[test]
    fn range_is_inclusive_and_bounded() {
        let mut rng = XorShift64::new(42);
        let mut seen = [false; 4];
        for _ in 0..1000 {
            let value = rng.range(3, 6);
            assert!((3..=6).contains(&value));
            seen[(value - 3) as usize] = true;
        }
        assert_eq!(seen, [true; 4]);
        assert_eq!(rng.range(5, 5), 5);
    }

    #[test]
    fn pick_returns_an_element() {
        let mut rng = XorShift64::new(42);
        let items = ["a", "b", "c"];
        for _ in 0..100 {
            assert!(items.contains(rng.pick(&items)));
        }
    }

    /// Pins the sequence: changing it changes every seeded fight.
    #[test]
    fn sequence_for_seed_42_is_stable() {
        let mut rng = XorShift64::new(42);
        let values: Vec<u64> = (0..4).map(|_| rng.next_u64()).collect();
        assert_eq!(
            values,
            vec![
                18108192690585582856,
                6830302529404445810,
                7514410519785295290,
                9589382499465979553
            ]
        );
    }
}

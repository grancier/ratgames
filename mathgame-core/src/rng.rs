//! A tiny deterministic pseudo-random generator (SplitMix64).
//!
//! Problem generation must be reproducible: the same seed yields the same
//! problems, so a drill replays identically and tests are deterministic. This is
//! dependency-free — the domain crate pulls in no `rand`.

use std::ops::RangeInclusive;

/// A seedable SplitMix64 generator. Deterministic: equal seeds produce equal
/// sequences.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// A generator seeded with `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64-bit value (SplitMix64).
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A uniform integer in the inclusive `range`. The tiny modulo bias is
    /// irrelevant at problem-generation scales.
    ///
    /// # Panics
    /// Panics if the range is empty (`start > end`).
    pub fn int_range(&mut self, range: RangeInclusive<i64>) -> i64 {
        let (low, high) = (*range.start(), *range.end());
        assert!(low <= high, "int_range called with an empty range");
        // Ranges are small (problem operands), so the width fits comfortably.
        let span = (high - low) as u64 + 1;
        low + (self.next_u64() % span) as i64
    }

    /// A fair coin flip.
    pub fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_seeds_produce_equal_sequences() {
        let mut a = Rng::new(12345);
        let mut b = Rng::new(12345);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn int_range_stays_within_bounds_and_covers_them() {
        let mut rng = Rng::new(99);
        let mut seen_low = false;
        let mut seen_high = false;
        for _ in 0..10_000 {
            let n = rng.int_range(3..=7);
            assert!((3..=7).contains(&n));
            seen_low |= n == 3;
            seen_high |= n == 7;
        }
        assert!(seen_low && seen_high, "range endpoints should be reachable");
    }

    #[test]
    fn int_range_handles_a_single_point() {
        let mut rng = Rng::new(0);
        assert_eq!(rng.int_range(5..=5), 5);
    }

    #[test]
    #[should_panic(expected = "empty range")]
    fn int_range_rejects_an_empty_range() {
        // Build the reversed range from values so the check happens at runtime.
        let (start, end) = (7_i64, 3_i64);
        Rng::new(0).int_range(start..=end);
    }
}

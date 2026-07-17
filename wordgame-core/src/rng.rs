//! A tiny deterministic pseudo-random generator (SplitMix64).
//!
//! Puzzle generation must be reproducible: the same seed yields the same
//! puzzles, so a drill replays identically and tests are deterministic. This is
//! dependency-free — the domain crate pulls in no `rand` — and uses the same
//! SplitMix64 recurrence as `mathgame-core`, so the sibling domains behave
//! alike without coupling to each other.

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

    /// A uniform index into a collection of `len` items. The tiny modulo bias
    /// is irrelevant at word-pool scales.
    ///
    /// # Panics
    /// Panics if `len` is zero (there is no index to draw).
    pub fn index(&mut self, len: usize) -> usize {
        assert!(len > 0, "index called with an empty collection");
        (self.next_u64() % len as u64) as usize
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
    fn index_stays_within_bounds_and_covers_them() {
        let mut rng = Rng::new(99);
        let mut seen = [false; 5];
        for _ in 0..10_000 {
            let i = rng.index(5);
            assert!(i < 5);
            seen[i] = true;
        }
        assert!(
            seen.iter().all(|&hit| hit),
            "every index should be drawable"
        );
    }

    #[test]
    fn index_handles_a_single_item() {
        assert_eq!(Rng::new(0).index(1), 0);
    }

    #[test]
    #[should_panic(expected = "empty collection")]
    fn index_rejects_an_empty_collection() {
        Rng::new(0).index(0);
    }
}

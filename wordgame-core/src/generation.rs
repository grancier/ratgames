//! Seeded, deterministic puzzle generation.
//!
//! A generator poses one level's shape — words whose length falls in a window,
//! each hiding a fixed number of letters — drawing uniformly from the pool.
//! Everything that can be wrong with the shape is rejected at construction, so
//! `generate` itself cannot fail; and generation draws only from the seeded
//! [`Rng`], so a level replays identically per seed.

use std::ops::RangeInclusive;

use crate::puzzle::Puzzle;
use crate::rng::Rng;
use crate::word_list::WordList;

/// The construction invariant behind `generate`'s expect: an eligible word is
/// at least two letters, blanks are distinct in-bounds positions, and at least
/// one letter stays visible (`blanks < shortest` at construction).
const GENERATION_INVARIANT: &str = "generated puzzles satisfy Puzzle's invariants by construction";

/// Errors building a [`PuzzleGenerator`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratorError {
    /// The length window is inverted (min > max).
    EmptyLengthRange { min: usize, max: usize },
    /// No pool word has a length inside the window.
    NoWordsInRange { min: usize, max: usize },
    /// A puzzle must hide at least one letter.
    NoBlanks,
    /// Hiding `blanks` letters from the shortest eligible word would leave
    /// nothing showing.
    TooManyBlanks { blanks: usize, shortest: usize },
}

impl std::fmt::Display for GeneratorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyLengthRange { min, max } => {
                write!(f, "the word-length window {min}..={max} is empty")
            }
            Self::NoWordsInRange { min, max } => {
                write!(f, "no pool word has a length in {min}..={max}")
            }
            Self::NoBlanks => write!(f, "a puzzle must hide at least one letter"),
            Self::TooManyBlanks { blanks, shortest } => write!(
                f,
                "cannot hide {blanks} letters of a {shortest}-letter word and still show one"
            ),
        }
    }
}

impl std::error::Error for GeneratorError {}

/// Poses puzzles of one level's shape: a uniform word from the eligible pool,
/// hiding `blanks` distinct positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PuzzleGenerator {
    /// The eligible words (uppercase), filtered from the pool at construction.
    words: Vec<String>,
    /// Hidden letters per puzzle — under every eligible word's length.
    blanks: usize,
}

impl PuzzleGenerator {
    /// A generator over `pool`'s words of a length in `lengths`, hiding
    /// `blanks` letters per puzzle.
    ///
    /// # Errors
    /// [`GeneratorError`] if the window is empty or matches no pool word, no
    /// letter would be hidden, or the blank count would blot out the shortest
    /// eligible word.
    pub fn new(
        pool: &WordList,
        lengths: RangeInclusive<usize>,
        blanks: usize,
    ) -> Result<Self, GeneratorError> {
        let (min, max) = (*lengths.start(), *lengths.end());
        if lengths.is_empty() {
            return Err(GeneratorError::EmptyLengthRange { min, max });
        }
        if blanks == 0 {
            return Err(GeneratorError::NoBlanks);
        }
        let words: Vec<String> = pool
            .with_lengths(lengths)
            .into_iter()
            .map(str::to_string)
            .collect();
        let Some(shortest) = words.iter().map(|word| word.len()).min() else {
            return Err(GeneratorError::NoWordsInRange { min, max });
        };
        if blanks >= shortest {
            return Err(GeneratorError::TooManyBlanks { blanks, shortest });
        }
        Ok(Self { words, blanks })
    }

    /// The next puzzle: a uniform word, then `blanks` distinct positions via a
    /// partial Fisher–Yates over the word's indices (sorted afterwards by
    /// [`Puzzle`]), so every position set is drawable and the sequence is
    /// deterministic per seed.
    #[must_use]
    pub fn generate(&self, rng: &mut Rng) -> Puzzle {
        let word = &self.words[rng.index(self.words.len())];
        let mut positions: Vec<usize> = (0..word.len()).collect();
        for drawn in 0..self.blanks {
            let swap = drawn + rng.index(positions.len() - drawn);
            positions.swap(drawn, swap);
        }
        positions.truncate(self.blanks);
        Puzzle::new(word.clone(), positions).expect(GENERATION_INVARIANT)
    }

    /// How many pool words are eligible for this shape.
    #[must_use]
    pub fn word_count(&self) -> usize {
        self.words.len()
    }

    /// Hidden letters per puzzle.
    #[must_use]
    pub fn blanks(&self) -> usize {
        self.blanks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> WordList {
        WordList::new(["cat", "dog", "sun", "bird", "fish", "apple", "crane"]).unwrap()
    }

    #[test]
    fn equal_seeds_generate_equal_puzzle_sequences() {
        let generator = PuzzleGenerator::new(&pool(), 3..=5, 1).unwrap();
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..200 {
            assert_eq!(generator.generate(&mut a), generator.generate(&mut b));
        }
    }

    #[test]
    fn different_seeds_diverge_somewhere() {
        let generator = PuzzleGenerator::new(&pool(), 3..=5, 1).unwrap();
        let (mut a, mut b) = (Rng::new(1), Rng::new(2));
        let diverged = (0..50).any(|_| generator.generate(&mut a) != generator.generate(&mut b));
        assert!(diverged, "different seeds should deal different puzzles");
    }

    #[test]
    fn every_puzzle_respects_the_shape() {
        let generator = PuzzleGenerator::new(&pool(), 4..=5, 2).unwrap();
        let mut rng = Rng::new(42);
        for _ in 0..500 {
            let puzzle = generator.generate(&mut rng);
            assert!((4..=5).contains(&puzzle.word().len()), "{puzzle:?}");
            assert_eq!(puzzle.blank_count(), 2);
            // At least one letter shows, and the mask matches the word length.
            let masked = puzzle.masked();
            assert_eq!(masked.chars().count(), puzzle.word().chars().count());
            assert!(masked.chars().any(|c| c != crate::puzzle::BLANK));
            assert_eq!(
                masked
                    .chars()
                    .filter(|&c| c == crate::puzzle::BLANK)
                    .count(),
                2
            );
        }
    }

    #[test]
    fn every_eligible_word_is_eventually_posed() {
        let generator = PuzzleGenerator::new(&pool(), 3..=3, 1).unwrap();
        assert_eq!(generator.word_count(), 3);
        let mut rng = Rng::new(9);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            seen.insert(generator.generate(&mut rng).word().to_string());
        }
        assert_eq!(seen.len(), 3, "CAT, DOG and SUN should all appear");
    }

    #[test]
    fn every_blank_position_is_eventually_drawn() {
        let generator = PuzzleGenerator::new(&pool(), 5..=5, 1).unwrap();
        let mut rng = Rng::new(11);
        let mut masks = std::collections::HashSet::new();
        for _ in 0..500 {
            masks.insert(generator.generate(&mut rng).masked());
        }
        // Two 5-letter words × 5 single-blank masks each.
        assert_eq!(masks.len(), 10, "every position should be drawable");
    }

    #[test]
    fn an_inverted_length_window_is_rejected() {
        // Build the reversed range from values so the emptiness is a runtime
        // fact (a literal `5..=3` trips clippy's reversed_empty_ranges).
        let (min, max) = (5_usize, 3_usize);
        assert_eq!(
            PuzzleGenerator::new(&pool(), min..=max, 1),
            Err(GeneratorError::EmptyLengthRange { min: 5, max: 3 })
        );
    }

    #[test]
    fn a_window_matching_no_words_is_rejected() {
        assert_eq!(
            PuzzleGenerator::new(&pool(), 8..=9, 1),
            Err(GeneratorError::NoWordsInRange { min: 8, max: 9 })
        );
    }

    #[test]
    fn zero_blanks_are_rejected() {
        assert_eq!(
            PuzzleGenerator::new(&pool(), 3..=5, 0),
            Err(GeneratorError::NoBlanks)
        );
    }

    #[test]
    fn a_blank_count_blotting_out_the_shortest_word_is_rejected() {
        // The 3..=5 window admits CAT: three blanks would hide all of it.
        assert_eq!(
            PuzzleGenerator::new(&pool(), 3..=5, 3),
            Err(GeneratorError::TooManyBlanks {
                blanks: 3,
                shortest: 3
            })
        );
        // Narrowed to 4..=5 the same count is fine (BIRD keeps a letter).
        assert!(PuzzleGenerator::new(&pool(), 4..=5, 3).is_ok());
    }
}

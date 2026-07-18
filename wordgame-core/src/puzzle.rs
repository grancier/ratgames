//! One posed word with hidden letters: the mask, the answer, and grading.

/// The masked-letter glyph a prompt shows for a hidden position: `"C_T"`.
pub const BLANK: char = '_';

/// Errors building a [`Puzzle`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PuzzleError {
    /// No positions were hidden — there would be nothing to answer.
    NoBlanks,
    /// Every letter was hidden — nothing of the word would show.
    AllBlank,
    /// A blank position pointed past the end of the word.
    OutOfBounds { position: usize, length: usize },
    /// The same position was hidden twice.
    DuplicatePosition(usize),
    /// The word is not a valid pool word (at least two ASCII letters).
    InvalidWord(String),
}

impl std::fmt::Display for PuzzleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoBlanks => write!(f, "a puzzle must hide at least one letter"),
            Self::AllBlank => write!(f, "a puzzle must leave at least one letter showing"),
            Self::OutOfBounds { position, length } => {
                write!(
                    f,
                    "blank position {position} is past a {length}-letter word"
                )
            }
            Self::DuplicatePosition(position) => {
                write!(f, "blank position {position} is hidden twice")
            }
            Self::InvalidWord(word) => {
                write!(f, "word {word:?} is not at least two ASCII letters")
            }
        }
    }
}

impl std::error::Error for PuzzleError {}

/// One missing-letter puzzle: a word and the positions hidden from it. The
/// prompt is the word with `_` at each hidden position; the answer is the
/// hidden letters in reading order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Puzzle {
    /// The full solution word, canonically UPPERCASE.
    word: String,
    /// The hidden positions: sorted, unique, in bounds, fewer than the word's
    /// letters (at least one letter always shows).
    blanks: Vec<usize>,
}

impl Puzzle {
    /// A puzzle hiding the `blanks` positions (any order) of `word`.
    ///
    /// # Errors
    /// [`PuzzleError`] if the word is not at least two ASCII letters, no
    /// position is hidden, a position repeats or is out of bounds, or every
    /// letter would be hidden.
    pub fn new(
        word: impl Into<String>,
        blanks: impl Into<Vec<usize>>,
    ) -> Result<Self, PuzzleError> {
        let word: String = word.into();
        if word.chars().count() < 2 || !word.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(PuzzleError::InvalidWord(word));
        }
        let word = word.to_ascii_uppercase();

        let mut blanks: Vec<usize> = blanks.into();
        blanks.sort_unstable();
        if blanks.is_empty() {
            return Err(PuzzleError::NoBlanks);
        }
        for pair in blanks.windows(2) {
            if pair[0] == pair[1] {
                return Err(PuzzleError::DuplicatePosition(pair[0]));
            }
        }
        // The word is ASCII by the check above, so bytes and letters agree.
        let length = word.len();
        if let Some(&position) = blanks.iter().find(|&&position| position >= length) {
            return Err(PuzzleError::OutOfBounds { position, length });
        }
        if blanks.len() >= length {
            return Err(PuzzleError::AllBlank);
        }
        Ok(Self { word, blanks })
    }

    /// The full solution word, UPPERCASE — what a miss reveals.
    #[must_use]
    pub fn word(&self) -> &str {
        &self.word
    }

    /// The prompt: the word with each hidden letter shown as [`BLANK`] —
    /// `"C_T"` for CAT hiding position 1.
    #[must_use]
    pub fn masked(&self) -> String {
        self.word
            .chars()
            .enumerate()
            .map(|(position, letter)| {
                if self.blanks.binary_search(&position).is_ok() {
                    BLANK
                } else {
                    letter
                }
            })
            .collect()
    }

    /// The letters the player must supply, in reading order — `"A"` for `C_T`.
    #[must_use]
    pub fn missing_letters(&self) -> String {
        // ASCII by construction, so byte indexing is letter indexing.
        let bytes = self.word.as_bytes();
        self.blanks
            .iter()
            .map(|&position| bytes[position] as char)
            .collect()
    }

    /// How many letters are hidden.
    #[must_use]
    pub fn blank_count(&self) -> usize {
        self.blanks.len()
    }

    /// Whether `answer` supplies exactly the missing letters, in reading
    /// order, case-insensitively. Surrounding whitespace is forgiven (an input
    /// field artefact); anything else must match letter for letter.
    #[must_use]
    pub fn grade(&self, answer: &str) -> bool {
        let answer = answer.trim();
        let expected = self.missing_letters();
        answer.chars().count() == expected.chars().count()
            && answer
                .chars()
                .zip(expected.chars())
                .all(|(typed, wanted)| typed.eq_ignore_ascii_case(&wanted))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn puzzle(word: &str, blanks: &[usize]) -> Puzzle {
        Puzzle::new(word, blanks.to_vec()).unwrap()
    }

    #[test]
    fn the_mask_hides_exactly_the_blank_positions() {
        assert_eq!(puzzle("cat", &[1]).masked(), "C_T");
        assert_eq!(puzzle("BIRD", &[0]).masked(), "_IRD");
        assert_eq!(puzzle("apple", &[1, 3]).masked(), "A_P_E");
    }

    #[test]
    fn the_mask_is_as_long_as_the_word() {
        let p = puzzle("banana", &[0, 2, 4]);
        assert_eq!(p.masked().chars().count(), p.word().chars().count());
        assert_eq!(p.blank_count(), 3);
    }

    #[test]
    fn missing_letters_come_in_reading_order_even_when_blanks_are_not() {
        let p = puzzle("crane", &[3, 0]);
        assert_eq!(p.masked(), "_RA_E");
        assert_eq!(p.missing_letters(), "CN");
    }

    #[test]
    fn the_word_is_canonically_uppercase() {
        assert_eq!(puzzle("cat", &[1]).word(), "CAT");
    }

    #[test]
    fn grading_accepts_the_missing_letters_in_any_case_and_trimmed() {
        let p = puzzle("cat", &[1]);
        assert!(p.grade("A"));
        assert!(p.grade("a"));
        assert!(p.grade(" a "));
    }

    #[test]
    fn grading_rejects_wrong_letters_wrong_lengths_and_blank_answers() {
        let p = puzzle("cat", &[1]);
        assert!(!p.grade("B"));
        assert!(!p.grade("AA"));
        assert!(!p.grade(""));
        assert!(!p.grade("é"));
    }

    #[test]
    fn grading_a_multi_blank_puzzle_requires_the_letters_in_order() {
        let p = puzzle("apple", &[1, 3]);
        assert_eq!(p.missing_letters(), "PL");
        assert!(p.grade("pl"));
        assert!(!p.grade("lp"));
        assert!(!p.grade("p"));
    }

    #[test]
    fn a_puzzle_must_hide_at_least_one_letter() {
        assert_eq!(Puzzle::new("cat", vec![]), Err(PuzzleError::NoBlanks));
    }

    #[test]
    fn a_puzzle_must_leave_a_letter_showing() {
        assert_eq!(
            Puzzle::new("cat", vec![0, 1, 2]),
            Err(PuzzleError::AllBlank)
        );
    }

    #[test]
    fn out_of_bounds_and_duplicate_positions_are_rejected() {
        assert_eq!(
            Puzzle::new("cat", vec![3]),
            Err(PuzzleError::OutOfBounds {
                position: 3,
                length: 3
            })
        );
        assert_eq!(
            Puzzle::new("cat", vec![1, 1]),
            Err(PuzzleError::DuplicatePosition(1))
        );
    }

    #[test]
    fn invalid_words_are_rejected() {
        for bad in ["a", "no1", "two words", ""] {
            assert_eq!(
                Puzzle::new(bad, vec![0]),
                Err(PuzzleError::InvalidWord(bad.to_string())),
                "{bad:?} should be rejected"
            );
        }
    }
}

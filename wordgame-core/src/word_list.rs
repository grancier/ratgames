//! The validated pool of words puzzles draw from.
//!
//! A word list is validated once, up front, and stored canonically UPPERCASE —
//! the game's display form. Validation is strict because the list is authored
//! config: a stray space, digit, or duplicate is a data mistake to surface at
//! load, not to sample around at play time.

use std::collections::HashSet;
use std::ops::RangeInclusive;

/// Errors building a [`WordList`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordListError {
    /// The list held no words at all.
    Empty,
    /// A word was under two letters — too short to hide one and show one.
    TooShort(String),
    /// A word held a character that is not an ASCII letter (punctuation,
    /// digits, accents, spaces).
    NotAlphabetic(String),
    /// The same word (case-insensitively) appeared more than once.
    Duplicate(String),
}

impl std::fmt::Display for WordListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "the word list holds no words"),
            Self::TooShort(word) => write!(f, "word {word:?} is under two letters"),
            Self::NotAlphabetic(word) => {
                write!(f, "word {word:?} holds a character that is not a letter")
            }
            Self::Duplicate(word) => write!(f, "word {word:?} appears more than once"),
        }
    }
}

impl std::error::Error for WordListError {}

/// The pool of words puzzles draw from: validated once, stored UPPERCASE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordList {
    words: Vec<String>,
}

impl WordList {
    /// Validate and canonicalise `words`: every word at least two ASCII
    /// letters, unique case-insensitively, stored uppercase in the given order.
    ///
    /// # Errors
    /// [`WordListError`] naming the first offending word, or [`WordListError::Empty`]
    /// for a wordless list.
    pub fn new<I, S>(words: I) -> Result<Self, WordListError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut canonical = Vec::new();
        let mut seen = HashSet::new();
        for word in words {
            let word = word.as_ref();
            if word.chars().count() < 2 {
                return Err(WordListError::TooShort(word.to_string()));
            }
            if !word.chars().all(|c| c.is_ascii_alphabetic()) {
                return Err(WordListError::NotAlphabetic(word.to_string()));
            }
            let upper = word.to_ascii_uppercase();
            if !seen.insert(upper.clone()) {
                return Err(WordListError::Duplicate(upper));
            }
            canonical.push(upper);
        }
        if canonical.is_empty() {
            return Err(WordListError::Empty);
        }
        Ok(Self { words: canonical })
    }

    /// Every word, uppercase, in list order.
    #[must_use]
    pub fn words(&self) -> &[String] {
        &self.words
    }

    /// Every word whose length falls inside `lengths`, in list order.
    #[must_use]
    pub fn with_lengths(&self, lengths: RangeInclusive<usize>) -> Vec<&str> {
        self.words
            .iter()
            .filter(|word| lengths.contains(&word.len()))
            .map(String::as_str)
            .collect()
    }

    /// How many words the pool holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// Whether the pool is empty — never true for a constructed list, kept for
    /// API completeness beside [`len`](Self::len).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_stored_uppercase_in_order() {
        let list = WordList::new(["cat", "Dog", "SUN"]).unwrap();
        assert_eq!(list.words(), ["CAT", "DOG", "SUN"]);
        assert_eq!(list.len(), 3);
        assert!(!list.is_empty());
    }

    #[test]
    fn an_empty_list_is_rejected() {
        let none: [&str; 0] = [];
        assert_eq!(WordList::new(none), Err(WordListError::Empty));
    }

    #[test]
    fn a_one_letter_word_is_rejected() {
        assert_eq!(
            WordList::new(["cat", "a"]),
            Err(WordListError::TooShort("a".to_string()))
        );
    }

    #[test]
    fn punctuation_digits_and_spaces_are_rejected() {
        for bad in ["it's", "no1", "two words", "café"] {
            assert_eq!(
                WordList::new([bad]),
                Err(WordListError::NotAlphabetic(bad.to_string())),
                "{bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn duplicates_are_rejected_case_insensitively() {
        assert_eq!(
            WordList::new(["cat", "dog", "CAT"]),
            Err(WordListError::Duplicate("CAT".to_string()))
        );
    }

    #[test]
    fn with_lengths_filters_by_the_inclusive_window() {
        let list = WordList::new(["cat", "bird", "apple", "banana"]).unwrap();
        assert_eq!(list.with_lengths(3..=4), ["CAT", "BIRD"]);
        assert_eq!(list.with_lengths(5..=5), ["APPLE"]);
        assert_eq!(list.with_lengths(6..=9), ["BANANA"]);
        assert!(list.with_lengths(7..=9).is_empty());
    }
}

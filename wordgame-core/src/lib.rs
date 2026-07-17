//! `wordgame-core` — the spelling domain for the missing-letter word game.
//!
//! This crate answers one question: *which word is posed, which of its letters
//! are hidden, and does a typed answer supply them?* It knows nothing about
//! sprites, banners, menus, framebuffers, storage, or config formats — and that
//! separation is enforced **structurally**: the crate has no dependencies, so
//! it *cannot* reach the presentation, persistence, or platform layers. Game
//! layers depend on the domain, never the reverse (the same boundary
//! `mathgame-core` draws for arithmetic).
//!
//! Modules:
//! - [`word_list`] — the validated pool of words puzzles draw from.
//! - [`puzzle`] — one posed word with hidden letters: mask, answer, grading.
//! - [`generation`] — seeded, deterministic puzzle generation per level shape.
//! - [`rng`] — a deterministic PRNG for reproducible generation.

pub mod generation;
pub mod puzzle;
pub mod rng;
pub mod word_list;

pub use generation::{GeneratorError, PuzzleGenerator};
pub use puzzle::{BLANK, Puzzle, PuzzleError};
pub use rng::Rng;
pub use word_list::{WordList, WordListError};

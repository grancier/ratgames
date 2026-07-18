//! `mazegame-core` — the maze domain for the number-collecting maze game.
//!
//! This crate answers one question: *what shape is the maze, where can the
//! player's block move, which numbers has it collected, and is the run won?*
//! It knows nothing about pixels, banners, framebuffers, key codes, storage,
//! or config formats — and that separation is enforced **structurally**: the
//! crate has no dependencies, so it *cannot* reach the presentation,
//! persistence, or platform layers. Game layers depend on the domain, never
//! the reverse (the same boundary `mathgame-core` and `wordgame-core` draw).
//!
//! The maze lives on a **tile grid**: every tile is either wall or floor, the
//! player's block occupies exactly one tile, and one step moves it one tile.
//! A consumer maps tiles to pixels (the shipped app draws 10px tiles, so the
//! bars are 10px wide and one step is one 10px move) — the domain never sees
//! pixel sizes.
//!
//! Modules:
//! - [`maze`] — the tile grid: seeded perfect-maze generation, authored maps.
//! - [`game`] — one run: block movement, wall collision, numbers, the win.
//! - [`rng`] — a deterministic PRNG for reproducible generation.

pub mod game;
pub mod maze;
pub mod rng;

pub use game::{Collectible, Direction, MazeGame, MazeGameError, Phase, StepOutcome};
pub use maze::{Maze, MazeError, Tile};
pub use rng::Rng;

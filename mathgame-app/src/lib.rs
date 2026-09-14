//! Mathgame composition: authored arithmetic, compiled campaigns, and seeded runs.
mod arithmetic;
mod formatting;
mod session;

pub use arithmetic::{Arithmetic, MathLevel, OperatorConfig, ProblemSpec};
pub use formatting::format_problem;
pub use session::{AttemptReport, MathgameCampaign, MathgameSession, MathgameSessionError};

#[cfg(test)]
mod test_support;

/// Fallback RNG seed for the problem sequence when the wall clock is unavailable.
/// Not a game rule — the arcade rules (lives, per-level goal, reward, and input
/// mode) come from the [`MathLevel`] gauntlet and the run-wide starting lives,
/// sourced from config.
pub const STARTER_SEED: u64 = 0x4d41_5448;

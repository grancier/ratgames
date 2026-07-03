//! Session state: the screen stack plus the player, score, level progression,
//! and arcade run a game tracks *around* the per-question [`Quiz`](crate::quiz::Quiz).
//!
//! [`Quiz`](crate::quiz::Quiz) grades one question; a session owns the
//! [`ScreenStack`] (title / menu / gameplay / pause), a [`PlayerProfile`], a
//! running [`Score`], [`LevelProgress`], and the [`Run`] that sequences the
//! arcade loop — [`Lives`], 1-ups, and game over — advancing to the next
//! question as answers land instead of ending at the quiz's terminal win.
//!
//! [`LevelGoal`] is standalone clearance machinery: it counts successes and
//! failures toward a per-level threshold and reports a [`LevelOutcome`] (in
//! progress / cleared / failed), independent of any particular game.

mod level_goal;
mod player;
mod progress;
mod run;
mod screen;

pub use level_goal::{LevelGoal, LevelGoalError, LevelOutcome};
pub use player::PlayerProfile;
pub use progress::{LevelProgress, Score};
pub use run::{Lives, Run, RunPhase};
pub use screen::{Screen, ScreenChange, ScreenStack};

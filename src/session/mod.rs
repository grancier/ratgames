//! Session state: the screen stack plus the player, score, level progression,
//! and arcade run a game tracks *around* the per-attempt challenge it poses.
//!
//! A game grades one attempt (in its own domain); a session owns the
//! [`ScreenStack`] (title / menu / gameplay / pause), a [`PlayerProfile`], a
//! running [`Score`], [`LevelProgress`], and the [`Run`] that sequences the
//! arcade loop — [`Lives`], 1-ups, and game over — advancing to the next
//! challenge as attempts land. [`GameRun`] ties these together, driven by a bare
//! success/failure so no game's content crosses in.
//!
//! [`LevelGoal`] is standalone clearance machinery: it counts successes and
//! failures toward a per-level threshold and reports a [`LevelOutcome`] (in
//! progress / cleared / failed), independent of any particular game.
//!
//! [`HighScores`] is a standalone ranked name/points table — a pure value type a
//! game records run outcomes into. [`JsonHighScoreStore`] is the optional
//! filesystem adapter that loads and saves one; the board itself stays pure.

mod game_run;
mod high_score_layout;
mod high_score_store;
mod high_scores;
mod level_goal;
mod player;
mod progress;
mod run;
mod screen;

pub use game_run::{
    AttemptOutcome, Campaign, CampaignError, GameRules, GameRulesError, GameRun, LevelSpec,
    LevelSpecError,
};
pub use high_score_layout::{HighScoreLayout, PlacedRow};
pub use high_score_store::{HighScoreStoreError, JsonHighScoreStore};
pub use high_scores::{HighScoreEntry, HighScores};
pub use level_goal::{LevelGoal, LevelGoalError, LevelOutcome};
pub use player::PlayerProfile;
pub use progress::{LevelProgress, Score};
pub use run::{Lives, Run, RunPhase};
pub use screen::{Screen, ScreenChange, ScreenStack};

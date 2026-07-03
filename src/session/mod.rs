//! Session state: the screen stack plus the player, score, and level progression
//! a game tracks *around* the per-question [`Quiz`](crate::quiz::Quiz).
//!
//! [`Quiz`](crate::quiz::Quiz) grades one question; a session owns the
//! [`ScreenStack`] (title / menu / gameplay / pause), a [`PlayerProfile`], a
//! running [`Score`], and [`LevelProgress`], advancing to the next question as
//! answers land instead of ending at the quiz's terminal win.

mod player;
mod progress;
mod screen;

pub use player::PlayerProfile;
pub use progress::{LevelProgress, Score};
pub use screen::{Screen, ScreenChange, ScreenStack};

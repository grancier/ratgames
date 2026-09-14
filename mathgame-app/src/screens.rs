//! Screen composition: shared context, menus, play, progression, and results.
mod context;
mod menus;
mod play;
mod progress;
mod results;

pub use context::Ctx;
pub use menus::title_screen;

#[cfg(test)]
mod test_support;

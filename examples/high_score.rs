//! `high_score` — read a high-score table from JSON and show the ranked board.
//!
//! A thin native main over [`demos::high_score`]: the demo lives in the
//! `demos` crate, shared with the browser build; this binary supplies the
//! storage backend (the [`JsonHighScoreStore`] file store, through the
//! [`HighScoreStore`] seam the web build satisfies with an in-memory store),
//! the font choice, the window, and the minifb frame loop. Run with
//! `cargo run --example high_score --features minifb`.

use anyhow::Result;
use demos::DemoCtx;
use ratgames::{
    FontConfig, HighScoreStore, JsonHighScoreStore, MinifbHost, ScreenStack, WindowConfig,
};

/// The board file, resolved at compile time so the example runs from any cwd.
const SCORES_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/high_scores.json");

fn main() -> Result<()> {
    // Load the ranked table (a plain JSON array of {name, points}) through the
    // storage seam — the backend is construction-time wiring, not board logic.
    let store: Box<dyn HighScoreStore> = Box::new(JsonHighScoreStore::new(SCORES_PATH));
    let scores = store.load()?;
    let screen = demos::high_score::build(&scores, &FontConfig::default())?;

    let window = WindowConfig {
        title: "ratgames: high scores".to_string(),
        width: Some(demos::high_score::VIRTUAL.w * 2),
        height: Some(demos::high_score::VIRTUAL.h * 2),
        ..WindowConfig::default()
    };
    let mut host = MinifbHost::new(&window, demos::high_score::presentation())?;
    let mut stack = ScreenStack::new(Box::new(screen));
    let mut ctx = DemoCtx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}

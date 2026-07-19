//! `mazegame-app` — a random-maze digit hunt on the ratgames toolkit.
//!
//! A seeded perfect maze of chunky bars fills the virtual screen; the player
//! steps a block through the corridors with the arrow keys — one keydown,
//! one tile — gathering the scattered digits **in order** (an out-of-turn
//! digit is solid, so bump, backtrack, and return). Once every digit is
//! gathered the exit door opens; stepping onto it clears the level, and the
//! seven-rung ladder climbs — smaller tiles, more digits, branchier mazes —
//! until clearing the top rung wins the run. Enter advances, `R` restarts
//! the level, `N` re-deals it, Esc quits.
//!
//! Every rule lives in the pure [`mazegame_core::MazeGame`]; this binary is
//! only the windowed shell. All tunables — the ladder (cells, tile size,
//! digits, branching per rung), the colours, the copy — come from
//! [`config::AppConfig`] (a bundled JSON default, or a `--config <path>`
//! override), never hardcoded here.

mod config;
mod scene;
mod screens;

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use ratgames::{MinifbHost, Presentation, ScreenStack, parse_config_flag};

use config::AppConfig;
use screens::{Ctx, PlayScreen};

/// Fallback seed for the maze deal when the wall clock is unavailable.
const STARTER_SEED: u64 = 0x4D41_5A45; // "MAZE"

fn main() -> Result<()> {
    let (config_path, _positionals) = parse_config_flag(std::env::args().skip(1))?;
    let config = AppConfig::resolve(config_path)?;

    // Vary the maze per run; fall back to the fixed starter seed.
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(STARTER_SEED);
    // The text glyph source (a 32px raster in the bundled config), resolved
    // once — it loads the font — and shared through the context.
    let glyphs = config.glyphs.resolve()?;
    let mut ctx = Ctx::new(&config, glyphs, seed)?;

    let screen = &config.engine.screen;
    let presentation = Presentation::new(
        screen.size,
        screen.backdrop,
        screen.letterbox,
        screen.min_scale,
    );
    let mut host = MinifbHost::new(&config.engine.window, presentation)?;
    let mut stack: ScreenStack<Ctx> = ScreenStack::new(Box::new(PlayScreen::new(&ctx)));

    // The host owns the frame loop; the app supplies only the quit condition.
    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;

    Ok(())
}

//! `mazegame-app` — a random-maze POC on the ratgames toolkit.
//!
//! A seeded perfect maze of 10px bars fills the virtual screen; the player
//! steps a 10px block through the corridors with the arrow keys — one keydown,
//! one tile — collecting the scattered digits. Once every digit is collected
//! the exit door opens, and stepping onto it wins the run. `R` deals a fresh
//! maze, Esc quits.
//!
//! Every rule lives in the pure [`mazegame_core::MazeGame`]; this binary is
//! only the windowed shell. All tunables — the maze shape, the 10px tile, the
//! colours, the copy — come from [`config::AppConfig`] (a bundled JSON
//! default, or a `--config <path>` override), never hardcoded here.

mod config;
mod scene;
mod screens;

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use mazegame_core::MazeGame;
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
    let game = MazeGame::new(
        config.maze.cells_w,
        config.maze.cells_h,
        config.maze.collectibles,
        seed,
    )?;
    // The text glyph source (a 32px raster in the bundled config), resolved
    // once — it loads the font — and shared through the context.
    let glyphs = config.glyphs.resolve()?;
    let mut ctx = Ctx::new(&config, glyphs, game, seed.wrapping_add(1));

    let screen = &config.engine.screen;
    let presentation = Presentation::new(
        screen.size,
        screen.backdrop,
        screen.letterbox,
        screen.min_scale,
    );
    let mut host = MinifbHost::new(&config.engine.window, presentation)?;
    let mut stack: ScreenStack<Ctx> = ScreenStack::new(Box::new(PlayScreen));

    // The host owns the frame loop; the app supplies only the quit condition.
    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;

    Ok(())
}

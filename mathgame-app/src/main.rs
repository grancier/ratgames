//! `mathgame-app` — the first playable: a tiny 8-bit arcade math quiz.
//!
//! Title → name entry → play → result, on a ratgames `ScreenStack` driven by the
//! native `MinifbHost`. Every rule lives in the pure
//! [`mathgame_app::MathgameSession`]; this binary is only the windowed shell.
//!
//! All tunables — the Menlo input font, its size, the banner/HUD scale and
//! shadow — come from [`config::AppConfig`] (a bundled JSON default, or a
//! `--config <path>` override), never hardcoded here.

mod config;
mod scores;
mod screens;

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use mathgame_app::MathgameSession;
use ratgames::{
    HighScores, InputField, MinifbHost, OverlayLayer, PixelLayer, Presentation, ScreenStack,
    SystemFont, parse_config_flag,
};

use config::AppConfig;
use screens::{Ctx, TitleScreen};

fn main() -> Result<()> {
    // Config from data: the bundled default, or a `--config <path>` TOML/JSON
    // override. No product value is hardcoded in this binary.
    let (config_path, _positionals) = parse_config_flag(std::env::args().skip(1))?;
    let AppConfig {
        engine,
        text,
        banner_glyphs,
        feedback,
        scores: scores_cfg,
    } = AppConfig::resolve(config_path)?;

    let font = SystemFont::load(&engine.input.font)?;
    let input = InputField::new(engine.input.clone(), font);

    // The one glyph source every pixel-art banner and the reject cross render
    // through — resolved once (it loads a font), then shared through the context.
    let glyphs = banner_glyphs.resolve()?;

    // The board persists across runs; a missing file is a fresh board, and a load
    // failure is non-fatal — warn and start empty rather than refuse to run.
    let board = scores::load(&scores_cfg.file).unwrap_or_else(|error| {
        eprintln!("warning: {error}; starting with an empty high-score board");
        HighScores::new()
    });

    // Vary the problem sequence per run; fall back to the fixed starter seed.
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(mathgame_app::STARTER_SEED);
    // The virtual screen size drives the banners' device-space layout (they
    // recover the integer fit factor from it), so thread it through the context.
    let screen = engine.screen;
    let virtual_size = screen.size;
    let mut ctx = Ctx::new(
        MathgameSession::with_seed(seed)?,
        input,
        text,
        glyphs,
        feedback,
        virtual_size,
        board,
        scores_cfg,
    );

    let presentation = Presentation::new(
        screen.size,
        screen.backdrop,
        screen.letterbox,
        screen.min_scale,
    );
    let mut host = MinifbHost::new(&engine.window, presentation)?;
    let mut stack: ScreenStack<Ctx> =
        ScreenStack::new(Box::new(TitleScreen::new(&*ctx.glyphs, text, virtual_size)));

    while host.is_open() && !ctx.quit {
        for event in host.poll_inputs() {
            stack.handle(event, &mut ctx);
        }
        stack.tick(&mut ctx);

        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        stack.collect_layers(&ctx, &mut world, &mut overlays);
        host.render(&world, &overlays)?;
    }

    Ok(())
}

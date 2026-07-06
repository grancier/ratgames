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
    InputField, JsonHighScoreStore, MinifbHost, Presentation, ScreenStack, SystemFont,
    parse_config_flag, take_levels_flag,
};

use config::AppConfig;
use screens::{Ctx, TitleScreen};

fn main() -> Result<()> {
    // Config from data: the bundled defaults, or overrides via `--levels <dir>`
    // (a directory of level_<n>.json) and `--config <path>` (run-wide TOML/JSON).
    // Pull the levels flag first, then parse `--config` from what remains. No
    // product value is hardcoded in this binary.
    let (levels_dir, rest) = take_levels_flag(std::env::args().skip(1))?;
    let (config_path, _positionals) = parse_config_flag(rest)?;
    let AppConfig {
        engine,
        text,
        banner_glyphs,
        feedback,
        timer_bar,
        interstitial,
        scores: scores_cfg,
        starting_lives,
        time_bonus_per_second,
        scoring,
    } = AppConfig::resolve(config_path)?;
    let levels = config::resolve_levels(levels_dir)?;

    let font = SystemFont::load(&engine.input.font)?;
    let input = InputField::new(engine.input.clone(), font);

    // The one glyph source every pixel-art banner and the reject cross render
    // through — resolved once (it loads a font), then shared through the context.
    let glyphs = banner_glyphs.resolve()?;

    // The board persists across runs through a JSON store bound to the config
    // path. A missing file is a fresh board, and a load failure is non-fatal —
    // warn and start empty rather than refuse to run.
    let store = JsonHighScoreStore::new(&scores_cfg.file);
    let board = scores::load_or_warn(&store);

    // Vary the problem sequence per run; fall back to the fixed starter seed.
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(mathgame_app::STARTER_SEED);
    // The virtual screen size drives the banners' device-space layout (they
    // recover the integer fit factor from it), so thread it through the context.
    let screen = engine.screen;
    let virtual_size = screen.size;
    // Frame rate: the host paces frames at this, so the question timer's frame
    // budget and the per-second time bonus are both measured against it.
    let frames_per_second = engine.window.target_fps as u32;
    let mut ctx = Ctx::new(
        MathgameSession::from_levels(&levels, starting_lives, seed)?.with_scoring(scoring)?,
        input,
        text,
        glyphs,
        feedback,
        timer_bar,
        interstitial,
        virtual_size,
        board,
        store,
        scores_cfg.capacity,
        frames_per_second,
        time_bonus_per_second,
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

    // The host owns the frame loop; the app supplies only the quit condition.
    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;

    Ok(())
}

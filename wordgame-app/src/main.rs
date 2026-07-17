//! `wordgame-app` — a tiny 8-bit arcade missing-letter speller.
//!
//! Title → difficulty select → name entry → play → result, on a ratgames
//! `ScreenStack` driven by the native `MinifbHost` — with an attract rotation
//! on the idle title and a continue prompt at game over. A word shows with
//! `_` at its hidden letters ("C_T"); the player types the missing letters,
//! one at a time, into the shared input field. Every rule lives in the pure
//! [`wordgame_app::WordgameSession`]; this binary is only the windowed shell.
//!
//! All tunables — the Menlo input font, its size, the banner/HUD scale and
//! shadow — come from [`config::AppConfig`] (a bundled JSON default, or a
//! `--config <path>` override), never hardcoded here. The gauntlet is
//! `config/levels/level_<n>.json` (or `--levels <dir>`), and the word pool is
//! `config/words.json`.

mod config;
mod scores;
mod screens;

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use ratgames::{
    InputField, JsonHighScoreStore, MinifbHost, Presentation, ScreenStack, SystemFont,
    parse_config_flag, take_levels_flag,
};
use wordgame_app::WordgameSession;

use config::AppConfig;
use screens::{Ctx, title_screen};

fn main() -> Result<()> {
    // Config from data: the bundled defaults, or overrides via `--levels <dir>`
    // (a directory of level_<n>.json) and `--config <path>` (run-wide
    // TOML/JSON). Pull the levels flag first, then parse `--config` from what
    // remains. No product value is hardcoded in this binary.
    let (levels_dir, rest) = take_levels_flag(std::env::args().skip(1))?;
    let (config_path, _positionals) = parse_config_flag(rest)?;
    let AppConfig {
        engine,
        text,
        banner_glyphs,
        hud_glyphs,
        feedback,
        timer_bar,
        interstitial,
        scores: scores_cfg,
        starting_lives,
        time_bonus_per_second,
        scoring,
        ranks,
        continues,
        continue_prompt,
        attract,
        difficulties,
        copy,
        layout,
    } = AppConfig::resolve(config_path)?;
    let levels = config::resolve_levels(levels_dir)?;
    let words = config::bundled_words();

    let font = SystemFont::load(&engine.input.font)?;
    let input = InputField::new(engine.input.clone(), font);

    // The glyph sources, resolved once (each loads a font) and shared through
    // the context: the display-height banner source, plus the optional smaller
    // body-text source (absent = share the banner source).
    let glyphs = banner_glyphs.resolve()?;
    let hud_glyphs = hud_glyphs.map(|cfg| cfg.resolve()).transpose()?;

    // The board persists across runs through a JSON store bound to the config
    // path. A missing file is a fresh board, and a load failure is non-fatal —
    // warn and start empty rather than refuse to run.
    let store = JsonHighScoreStore::new(&scores_cfg.file);
    let board = scores::load_or_warn(&store);

    // Vary the puzzle sequence per run; fall back to the fixed starter seed.
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(wordgame_app::STARTER_SEED);
    // The virtual screen size drives the banners' device-space layout (they
    // recover the integer fit factor from it), so thread it through the
    // context.
    let screen = engine.screen;
    let virtual_size = screen.size;
    // Frame rate: the host paces frames at this, so the question timer's frame
    // budget and the per-second time bonus are both measured against it.
    let frames_per_second = engine.window.target_fps as u32;
    let mut ctx = Ctx {
        session: WordgameSession::from_levels(&levels, &words, starting_lives, seed)?
            .with_scoring(scoring.clone())?
            .with_continues(continues),
        input,
        text,
        glyphs,
        hud_glyphs,
        feedback,
        timer_bar,
        interstitial,
        virtual_size,
        scores: board,
        store,
        capacity: scores_cfg.capacity,
        frames_per_second,
        time_bonus_per_second,
        ranks,
        continue_prompt,
        attract,
        difficulties,
        copy,
        layout,
        levels,
        words,
        scoring,
        continues,
        // A difficulty rebuild deals a fresh puzzle sequence, not a replay of
        // the startup session's.
        next_seed: seed.wrapping_add(1),
        quit: false,
    };

    let presentation = Presentation::new(
        screen.size,
        screen.backdrop,
        screen.letterbox,
        screen.min_scale,
    );
    let mut host = MinifbHost::new(&engine.window, presentation)?;
    let mut stack: ScreenStack<Ctx> = ScreenStack::new(title_screen(&ctx));

    // The host owns the frame loop; the app supplies only the quit condition.
    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;

    Ok(())
}

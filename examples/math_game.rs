//! `math_game` — a worked example wiring the ratgames toolkit into a tiny quiz.
//!
//! A thin native main over [`demos::math_game`] (the `Challenge` driver, its
//! screens, and the demo-local quiz): the demo lives in the `demos` crate,
//! shared with the browser build; this binary supplies the config, the window,
//! and the minifb frame loop. Run with
//! `cargo run --example math_game --features minifb`; type an answer, Enter
//! submits, Backspace edits, Esc (or close) quits. From the win / game-over
//! card, Enter restarts. Pass `--config <file>` to load a TOML/JSON `Config`
//! for the window / screen / input styling.

use anyhow::Result;
use ratgames::{ConfigSource, MinifbHost, Presentation, ScreenStack, parse_config_flag};

fn main() -> Result<()> {
    let (config_path, _) = parse_config_flag(std::env::args().skip(1))?;
    let config = ConfigSource::resolve(config_path).load()?;

    let mut ctx = demos::math_game::context(&config)?;

    // The host owns the window, framebuffer, and per-frame loop; hand it a ready
    // presentation over the configured (fixed) virtual screen.
    let s = config.screen;
    let presentation = Presentation::new(s.size, s.backdrop, s.letterbox, s.min_scale);
    let mut host = MinifbHost::new(&config.window, presentation)?;
    let mut stack = ScreenStack::new(demos::math_game::challenge_screen(&ctx));

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}

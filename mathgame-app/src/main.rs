//! `mathgame-app` — the first playable: a tiny 8-bit arcade math quiz.
//!
//! Title → name entry → play → result, on a ratgames `ScreenStack` driven by the
//! native `MinifbHost`. Every rule lives in the pure
//! [`mathgame_app::MathgameSession`]; this binary is only the windowed shell.

mod screens;

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use mathgame_app::MathgameSession;
use ratgames::{
    Config, InputField, MinifbHost, OverlayLayer, PixelLayer, Presentation, ScreenStack, SystemFont,
};

use screens::{Ctx, TitleScreen};

fn main() -> Result<()> {
    let config = Config::default();
    let font = SystemFont::load(&config.input.font)?;
    let input = InputField::new(config.input.clone(), font);

    // Vary the problem sequence per run; fall back to the fixed starter seed.
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(mathgame_app::STARTER_SEED);
    let mut ctx = Ctx::new(MathgameSession::with_seed(seed)?, input);

    let screen = config.screen;
    let presentation = Presentation::new(
        screen.size,
        screen.backdrop,
        screen.letterbox,
        screen.min_scale,
    );
    let mut host = MinifbHost::new(&config.window, presentation)?;
    let mut stack: ScreenStack<Ctx> = ScreenStack::new(Box::new(TitleScreen::new()));

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

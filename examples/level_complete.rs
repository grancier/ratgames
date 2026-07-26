//! `level_complete` — press Enter to reveal a 32px, drop-shadowed "YOU WIN!"
//! banner centred on the screen.
//!
//! A minimal showcase of the reusable card machinery: two chained
//! [`PromptScreen`]s. The first holds a "PRESS ENTER" prompt; confirming it
//! [`Replace`](ScreenChange::Replace)s it with the "YOU WIN!" card, and either
//! Enter or Esc from there quits. Each banner is a [`ShadowBanner`] baked once
//! through a [`RasterGlyphSource`] by a [`ShadowBannerFactory`]; the
//! [`ScreenStack`] is driven by [`MinifbHost::run`]. Only ratgames. Run with
//! `cargo run --example level_complete --features minifb`; Enter reveals the
//! banner, Esc (or close) quits.
//!
//! [`ShadowBanner`]: ratgames::ShadowBanner

use anyhow::Result;
use ratgames::{
    Color, FontConfig, MinifbHost, Presentation, PromptExit, PromptScreen, RasterGlyphSource,
    ScreenChange, ScreenStack, ShadowBannerFactory, ShadowStyle, Size, SystemFont, WindowConfig,
};

const VIRTUAL: Size = Size { w: 640, h: 360 };
const BACKDROP: Color = Color::rgb(0x10, 0x12, 0x28);

/// The one durable bit of state the host loop watches.
#[derive(Default)]
struct Ctx {
    quit: bool,
}

fn main() -> Result<()> {
    // A 32px raster source (the system's default monospace); the two banners
    // bake through it once, so the source is not kept.
    let source = RasterGlyphSource::new(SystemFont::load(&FontConfig::default())?, 32);
    let factory = ShadowBannerFactory::new(&source, ShadowStyle::default(), VIRTUAL);
    let prompt = factory.centered("PRESS ENTER", 1);
    let win = factory.centered("YOU WIN!", 1);

    // The win card: any exit quits (a terminal card holds nothing further).
    let win_screen = PromptScreen::new(vec![win], |exit, ctx: &mut Ctx| {
        match exit {
            PromptExit::Confirmed | PromptExit::Cancelled => ctx.quit = true,
            PromptExit::Idled => {}
        }
        ScreenChange::None
    });
    // The prompt card: Enter reveals the win card, Esc quits.
    let prompt_screen = PromptScreen::new(vec![prompt], |exit, ctx: &mut Ctx| match exit {
        PromptExit::Confirmed => ScreenChange::Replace(Box::new(win_screen)),
        PromptExit::Cancelled => {
            ctx.quit = true;
            ScreenChange::None
        }
        PromptExit::Idled => ScreenChange::None,
    });

    let window = WindowConfig {
        title: "ratgames: level complete".to_string(),
        width: Some(VIRTUAL.w * 2),
        height: Some(VIRTUAL.h * 2),
        ..WindowConfig::default()
    };
    let presentation = Presentation::new(VIRTUAL, BACKDROP, Color::rgb(0, 0, 0), 1);
    let mut host = MinifbHost::new(&window, presentation)?;
    let mut stack = ScreenStack::new(Box::new(prompt_screen));
    let mut ctx = Ctx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}

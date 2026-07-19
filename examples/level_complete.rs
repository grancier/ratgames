//! `level_complete` — press Enter to reveal a 32px, drop-shadowed "YOU WIN!"
//! banner centred on the screen.
//!
//! A minimal showcase of the reusable [`ShadowBanner`]: a pixel-art text banner
//! with a real drop shadow, baked once through a [`RasterGlyphSource`] and shown
//! on a one-screen [`ScreenStack`] driven by [`MinifbHost::run`]. Only ratgames.
//! Run with `cargo run --example level_complete --features minifb`; Enter reveals
//! the banner, Esc (or close) quits.

use anyhow::Result;
use ratgames::{
    Color, FontConfig, MinifbHost, OverlayLayer, PixelLayer, Presentation, RasterGlyphSource,
    Screen, ScreenChange, ScreenStack, ShadowBanner, ShadowBannerFactory, ShadowStyle, Size,
    SystemFont, UiInput, WindowConfig,
};

const VIRTUAL: Size = Size { w: 640, h: 360 };
const BACKDROP: Color = Color::rgb(0x10, 0x12, 0x28);

/// The one durable bit of state the host loop watches.
#[derive(Default)]
struct Ctx {
    quit: bool,
}

/// Shows a "PRESS ENTER" prompt until Enter is pressed, then the "YOU WIN!"
/// banner. Both are pre-baked; `shown` picks which one this frame.
struct WinScreen {
    prompt: ShadowBanner,
    win: ShadowBanner,
    shown: bool,
}

impl Screen<Ctx> for WinScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Confirm => self.shown = true,
            UiInput::Cancel => ctx.quit = true,
            _ => {}
        }
        ScreenChange::None
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        _world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        overlays.push(if self.shown { &self.win } else { &self.prompt });
    }
}

fn main() -> Result<()> {
    // A 32px raster source (the system's default monospace); the two banners
    // bake through it once, so the source is not kept.
    let source = RasterGlyphSource::new(SystemFont::load(&FontConfig::default())?, 32);
    let factory = ShadowBannerFactory::new(&source, ShadowStyle::default(), VIRTUAL);
    let prompt = factory.centered("PRESS ENTER", 1);
    let win = factory.centered("YOU WIN!", 1);

    let window = WindowConfig {
        title: "ratgames: level complete".to_string(),
        width: Some(VIRTUAL.w * 2),
        height: Some(VIRTUAL.h * 2),
        ..WindowConfig::default()
    };
    let presentation = Presentation::new(VIRTUAL, BACKDROP, Color::rgb(0, 0, 0), 1);
    let mut host = MinifbHost::new(&window, presentation)?;
    let mut stack = ScreenStack::new(Box::new(WinScreen {
        prompt,
        win,
        shown: false,
    }));
    let mut ctx = Ctx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}

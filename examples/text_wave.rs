//! `text_wave` — the ratgames [`TextWave`] effect: a line of big pixel-art text
//! that ripples up and back down, composited through the integer upscale.
//!
//! The wave is a plain `ratgames` [`PixelLayer`] over a [`RasterGlyphSource`],
//! driven by [`MinifbHost::run`] on a [`ScreenStack`] — the toolkit owns the
//! window loop, so this example is just the effect. Run with
//! `cargo run --example text_wave --features minifb`; Esc (or close) quits.

use anyhow::Result;
use ratgames::{
    Color, FontFamily, FontSource, FontStretch, FontStyle, FontWeight, MinifbHost, OverlayLayer,
    PixelLayer, Presentation, RasterGlyphSource, Screen, ScreenChange, ScreenStack, Size,
    SystemFont, TextWave, UiInput, WindowConfig,
};

/// The virtual screen the wave composes into; the window integer-upscales it.
const VIRTUAL: Size = Size { w: 640, h: 360 };
/// The retro navy backdrop and the green ink, echoing the prototype's palette.
const BACKDROP: Color = Color::rgb(0x18, 0x18, 0x30);
const INK: Color = Color::rgb(0x39, 0xD3, 0x53);

/// The one durable bit of state: a quit flag the host loop watches.
#[derive(Default)]
struct Ctx {
    quit: bool,
}

/// One screen: it owns the wave (its local view state) and steps it each frame.
struct WaveScreen {
    wave: TextWave,
}

impl Screen<Ctx> for WaveScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        if matches!(input, UiInput::Cancel) {
            ctx.quit = true;
        }
        ScreenChange::None
    }

    fn tick(&mut self, _ctx: &mut Ctx) -> ScreenChange<Ctx> {
        self.wave.advance();
        ScreenChange::None
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        world: &mut Vec<&'a dyn PixelLayer>,
        _overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        world.push(&self.wave);
    }
}

fn main() -> Result<()> {
    // A crisp, high-resolution glyph source (Menlo bold at 48 source px); the
    // wave bakes each letter through it once, so the font is not kept.
    let font = SystemFont::from_source(&FontSource::System {
        family: FontFamily::Named("Menlo".to_string()),
        weight: FontWeight(700),
        style: FontStyle::Normal,
        stretch: FontStretch::Normal,
    })?;
    let source = RasterGlyphSource::new(font, 48);
    let wave = TextWave::new(&source, "PERFECT!", INK);

    let window = WindowConfig {
        title: "ratgames: text wave".to_string(),
        width: Some(VIRTUAL.w * 2),
        height: Some(VIRTUAL.h * 2),
        ..WindowConfig::default()
    };
    let presentation = Presentation::new(VIRTUAL, BACKDROP, Color::rgb(0, 0, 0), 1);
    let mut host = MinifbHost::new(&window, presentation)?;
    let mut stack = ScreenStack::new(Box::new(WaveScreen { wave }));
    let mut ctx = Ctx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}

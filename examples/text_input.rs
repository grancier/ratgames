//! `text_input` — type into the field; press Enter to show it as a big banner.
//!
//! A showcase of the reusable [`InputField`] (the same anti-aliased text-entry
//! overlay the games use): typing edits the field, and Enter bakes the submitted
//! line into a centred [`ShadowBanner`] through a [`ShadowBannerFactory`]. Only
//! ratgames. Run with `cargo run --example text_input --features minifb`; type,
//! Enter shows it, Backspace edits, Esc quits.

use anyhow::Result;
use ratgames::{
    Color, FontConfig, InputConfig, InputField, MinifbHost, OverlayLayer, PixelLayer, Presentation,
    RasterGlyphSource, Screen, ScreenChange, ScreenStack, ShadowBanner, ShadowBannerFactory,
    ShadowStyle, Size, SystemFont, UiInput, WindowConfig,
};

const VIRTUAL: Size = Size { w: 640, h: 360 };
const BACKDROP: Color = Color::rgb(0x10, 0x12, 0x28);

/// The one durable bit of state the host loop watches.
#[derive(Default)]
struct Ctx {
    quit: bool,
}

/// The text-entry field plus the banner baked from the last submitted line. It
/// owns the glyph source so it can re-bake the banner on each Enter.
struct InputScreen {
    input: InputField,
    source: RasterGlyphSource,
    banner: Option<ShadowBanner>,
}

impl Screen<Ctx> for InputScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Char(ch) => self.input.type_char(ch),
            UiInput::Backspace => self.input.backspace(),
            UiInput::Confirm => {
                let text = self.input.submit();
                if !text.trim().is_empty() {
                    let banner =
                        ShadowBannerFactory::new(&self.source, ShadowStyle::default(), VIRTUAL)
                            .centered(&text, 1);
                    self.banner = Some(banner);
                }
            }
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
        if let Some(banner) = &self.banner {
            overlays.push(banner);
        }
        overlays.push(&self.input);
    }
}

fn main() -> Result<()> {
    // One font for the field's anti-aliased text, another for the banner's raster
    // glyphs (`SystemFont` isn't `Clone`; loading the default monospace twice is
    // cheap).
    let source = RasterGlyphSource::new(SystemFont::load(&FontConfig::default())?, 40);
    let input = InputField::new(
        InputConfig::default(),
        SystemFont::load(&FontConfig::default())?,
    )
    .with_prompt("TYPE, THEN ENTER: ");

    let window = WindowConfig {
        title: "ratgames: text input".to_string(),
        width: Some(VIRTUAL.w * 2),
        height: Some(VIRTUAL.h * 2),
        ..WindowConfig::default()
    };
    let presentation = Presentation::new(VIRTUAL, BACKDROP, Color::rgb(0, 0, 0), 1);
    let mut host = MinifbHost::new(&window, presentation)?;
    let mut stack = ScreenStack::new(Box::new(InputScreen {
        input,
        source,
        banner: None,
    }));
    let mut ctx = Ctx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}

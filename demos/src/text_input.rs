//! `text_input` — type into the field; press Enter to show it as a big banner.
//!
//! A showcase of the reusable [`InputField`] (the same anti-aliased text-entry
//! overlay the games use): typing edits the field, and Enter bakes the
//! submitted line into a centred [`ShadowBanner`] through a
//! [`ShadowBannerFactory`].

use ratgames::{
    Color, FontConfig, InputConfig, InputField, OverlayLayer, PixelLayer, Presentation,
    RasterGlyphSource, Screen, ScreenChange, ShadowBanner, ShadowBannerFactory, ShadowStyle, Size,
    SystemFont, UiInput,
};

use crate::{DemoCtx, DemoError};

/// The virtual screen the demo composes into; a host integer-upscales it.
pub const VIRTUAL: Size = Size { w: 640, h: 360 };
const BACKDROP: Color = Color::rgb(0x10, 0x12, 0x28);
/// Source-pixel height the banner rasterises at.
const CELL_PX: u32 = 40;

/// The compositor for the demo's fixed virtual screen.
#[must_use]
pub fn presentation() -> Presentation {
    Presentation::new(VIRTUAL, BACKDROP, Color::rgb(0, 0, 0), 1)
}

/// The text-entry field plus the banner baked from the last submitted line. It
/// owns the glyph source so it can re-bake the banner on each Enter.
pub struct InputScreen {
    input: InputField,
    source: RasterGlyphSource,
    banner: Option<ShadowBanner>,
}

/// The demo: an empty field prompting for text, and the banner source. `font`
/// styles both the field's anti-aliased text and the banner's raster glyphs
/// (`SystemFont` isn't `Clone`; loading the face twice is cheap).
///
/// # Errors
/// [`DemoError::Font`] if `font` cannot be loaded.
pub fn build(font: &FontConfig) -> Result<InputScreen, DemoError> {
    let source = RasterGlyphSource::new(SystemFont::load(font)?, CELL_PX);
    let input = InputField::new(InputConfig::default(), SystemFont::load(font)?)
        .with_prompt("TYPE, THEN ENTER: ");
    Ok(InputScreen {
        input,
        source,
        banner: None,
    })
}

impl Screen<DemoCtx> for InputScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut DemoCtx) -> ScreenChange<DemoCtx> {
        match input {
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
            // Everything else is line editing (type, backspace, forward delete,
            // caret movement); the field ignores events it does not own.
            other => {
                self.input.handle(other);
            }
        }
        ScreenChange::None
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a DemoCtx,
        _world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        if let Some(banner) = &self.banner {
            overlays.push(banner);
        }
        overlays.push(&self.input);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_bakes_a_banner_on_submit_with_the_embedded_face() {
        let font = FontConfig::default().with_embedded_font();
        let mut screen = build(&font).expect("the embedded face always loads");
        let mut ctx = DemoCtx::default();
        for ch in "HI".chars() {
            screen.handle(UiInput::Char(ch), &mut ctx);
        }
        screen.handle(UiInput::Confirm, &mut ctx);

        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        screen.collect_layers(&ctx, &mut world, &mut overlays);
        assert_eq!(overlays.len(), 2, "the baked banner and the field");
    }
}

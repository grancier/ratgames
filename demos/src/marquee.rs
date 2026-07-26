//! `marquee` — a scrolling oversized-text banner over an anti-aliased input
//! field.
//!
//! The banner ([`Marquee`], pixel-art world) and the field ([`InputField`],
//! device-space overlay) compose through the ratgames [`Config`]: the marquee's
//! glyph source, palette, and speed and the field's font and styling all flow
//! from it, so a `--config` file (or the web build's embedded-font swap)
//! restyles the demo without touching this code.

use ratgames::{
    Config, FontFamily, FontSource, FontStretch, FontStyle, FontWeight, GlyphSourceConfig,
    InputField, Marquee, OverlayLayer, PixelLayer, Screen, ScreenChange, Size, SystemFont, UiInput,
};

use crate::{DemoCtx, DemoError};

/// The virtual screen the demo's preset composes into — the same 640×360 the
/// fixed-size demos use, so the input field renders at exactly the device
/// scale `text_input`'s does.
pub const VIRTUAL: Size = Size { w: 640, h: 360 };
/// Source-pixel height of the preset's banner glyphs, sized so the capitals
/// stand roughly 80% of the screen height — display height from the *source*,
/// not from magnification (`scale` ≠ resolution), so the letters stay as
/// crisply defined as the 32px body text.
const BANNER_CELL_PX: u32 = 380;

/// The demo's own preset over the neutral library defaults: the shared virtual
/// screen, and the banner baked through a crisp screen-filling raster source
/// (generic monospace, bold — no named family) at source-scale 1, instead of
/// the chunky 8×8 bitmap magnified sixfold. The outline/shadow/tracking values
/// re-tune the default look to the finer source pixels. A `--config` file
/// still overrides all of it.
#[must_use]
pub fn default_config() -> Config {
    let mut config = Config::default();
    config.screen.size = VIRTUAL;
    config.marquee.text_scale = 1;
    config.marquee.tracking = 8;
    config.marquee.shadow_depth = 28;
    config.marquee.outline_px = 6;
    config.marquee.gap = 120;
    config.marquee.glyph_source = GlyphSourceConfig::Raster {
        cell_px: BANNER_CELL_PX,
        threshold: 128,
        font: FontSource::System {
            family: FontFamily::Default,
            weight: FontWeight(700),
            style: FontStyle::Normal,
            stretch: FontStretch::Normal,
        },
    };
    config
}

/// The whole demo as one screen: the scrolling banner and the input field. It
/// owns both, scrolls the banner each tick, and routes typing into the field.
pub struct MarqueeScreen {
    marquee: Marquee,
    input: InputField,
}

/// The demo: `text` baked through `config`'s marquee style, over `config`'s
/// input field.
///
/// # Errors
/// [`DemoError::Config`] if the banner cannot be baked (an oversized sprite, a
/// failing glyph font); [`DemoError::Font`] if the field's font cannot load.
pub fn build(config: &Config, text: &str) -> Result<MarqueeScreen, DemoError> {
    let marquee = Marquee::new(config.marquee.text_sprite(text)?, config.marquee.speed);
    let input = InputField::new(config.input.clone(), SystemFont::load(&config.input.font)?);
    Ok(MarqueeScreen { marquee, input })
}

impl Screen<DemoCtx> for MarqueeScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut DemoCtx) -> ScreenChange<DemoCtx> {
        match input {
            UiInput::Confirm => {
                self.input.submit();
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

    fn tick(&mut self, _ctx: &mut DemoCtx) -> ScreenChange<DemoCtx> {
        self.marquee.advance();
        ScreenChange::None
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a DemoCtx,
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        world.push(&self.marquee);
        overlays.push(&self.input);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_builds_deterministically_with_embedded_fonts() {
        // The web build's exact construction: the demo preset with every font
        // swapped for the crate-bundled face.
        let mut config = default_config();
        config.input.font = config.input.font.with_embedded_font();
        config.marquee.glyph_source = config.marquee.glyph_source.with_embedded_font();
        assert!(
            build(&config, "YOU WIN!!").is_ok(),
            "the embedded face always loads"
        );
    }

    #[test]
    fn the_preset_banner_is_raster_at_source_scale_one() {
        let config = default_config();
        assert_eq!(config.screen.size, VIRTUAL);
        assert_eq!(
            config.marquee.text_scale, 1,
            "the resolution lives in the source, not the magnification"
        );
        assert!(
            matches!(
                config.marquee.glyph_source,
                GlyphSourceConfig::Raster {
                    cell_px: BANNER_CELL_PX,
                    ..
                }
            ),
            "the banner bakes through the 64px raster source"
        );
    }
}

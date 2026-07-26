//! `marquee` — a scrolling oversized-text banner over an anti-aliased input
//! field.
//!
//! The banner ([`Marquee`], pixel-art world) and the field ([`InputField`],
//! device-space overlay) compose through the ratgames [`Config`]: the marquee's
//! glyph source, palette, and speed and the field's font and styling all flow
//! from it, so a `--config` file (or the web build's embedded-font swap)
//! restyles the demo without touching this code.

use ratgames::{
    Config, InputField, Marquee, OverlayLayer, PixelLayer, Screen, ScreenChange, SystemFont,
    UiInput,
};

use crate::{DemoCtx, DemoError};

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
    fn builds_deterministically_from_an_embedded_font_config() {
        let mut config = Config::default();
        config.input.font = config.input.font.with_embedded_font();
        // The default marquee glyph source is the font-free 8x8 bitmap; the
        // swap leaves it unchanged, so the whole build is deterministic.
        config.marquee.glyph_source = config.marquee.glyph_source.with_embedded_font();
        assert!(
            build(&config, "YOU WIN!!").is_ok(),
            "the embedded face always loads"
        );
    }
}

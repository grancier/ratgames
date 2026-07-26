//! `text_wave` — the ratgames [`TextWave`] effect: a line of big pixel-art text
//! that ripples up and back down, composited through the integer upscale.
//!
//! The wave is a plain ratgames [`PixelLayer`] over a [`RasterGlyphSource`];
//! the host owns the frame loop, so this demo is just the effect. Which font
//! face the letters bake through is the caller's: the native example passes a
//! system face, the web build the crate-bundled embedded one.

use ratgames::{
    Color, FontSource, OverlayLayer, PixelLayer, Presentation, RasterGlyphSource, Screen,
    ScreenChange, Size, SystemFont, TextWave, UiInput,
};

use crate::{DemoCtx, DemoError};

/// The virtual screen the wave composes into; a host integer-upscales it.
pub const VIRTUAL: Size = Size { w: 640, h: 360 };
/// The retro navy backdrop and the green ink, echoing the prototype's palette.
const BACKDROP: Color = Color::rgb(0x18, 0x18, 0x30);
const INK: Color = Color::rgb(0x39, 0xD3, 0x53);
/// Source-pixel height the wave's letters rasterise at (crisp, hi-res glyphs).
const CELL_PX: u32 = 48;

/// The compositor for the demo's fixed virtual screen.
#[must_use]
pub fn presentation() -> Presentation {
    Presentation::new(VIRTUAL, BACKDROP, Color::rgb(0, 0, 0), 1)
}

/// One screen: it owns the wave (its local view state) and steps it each frame.
pub struct WaveScreen {
    wave: TextWave,
}

/// The demo, its letters baked once through `font` at the wave's cell size (so
/// the font is not kept).
///
/// # Errors
/// [`DemoError::Font`] if `font` cannot be loaded.
pub fn build(font: &FontSource) -> Result<WaveScreen, DemoError> {
    let source = RasterGlyphSource::new(SystemFont::from_source(font)?, CELL_PX);
    Ok(WaveScreen {
        wave: TextWave::new(&source, "PERFECT!", INK),
    })
}

impl Screen<DemoCtx> for WaveScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut DemoCtx) -> ScreenChange<DemoCtx> {
        if matches!(input, UiInput::Cancel) {
            ctx.quit = true;
        }
        ScreenChange::None
    }

    fn tick(&mut self, _ctx: &mut DemoCtx) -> ScreenChange<DemoCtx> {
        self.wave.advance();
        ScreenChange::None
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a DemoCtx,
        world: &mut Vec<&'a dyn PixelLayer>,
        _overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        world.push(&self.wave);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratgames::FontWeight;

    #[test]
    fn builds_deterministically_from_the_embedded_face() {
        let screen = build(&FontSource::Embedded {
            weight: FontWeight(700),
        })
        .expect("the embedded face always loads");
        let ctx = DemoCtx::default();
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        screen.collect_layers(&ctx, &mut world, &mut overlays);
        assert_eq!(world.len(), 1, "the wave is the world");
    }
}

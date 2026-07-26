//! `level_complete` — press Enter to reveal a 32px, drop-shadowed "YOU WIN!"
//! banner centred on the screen.
//!
//! A minimal showcase of the reusable card machinery: two chained
//! [`PromptScreen`]s. The first holds a "PRESS ENTER" prompt; confirming it
//! [`Replace`](ScreenChange::Replace)s it with the "YOU WIN!" card, and either
//! Enter or Esc from there quits. Each banner is a
//! [`ShadowBanner`](ratgames::ShadowBanner) baked once through a
//! [`RasterGlyphSource`] by a [`ShadowBannerFactory`].

use ratgames::{
    Color, FontConfig, Presentation, PromptExit, PromptScreen, RasterGlyphSource, ScreenChange,
    ShadowBannerFactory, ShadowStyle, Size, SystemFont,
};

use crate::{DemoCtx, DemoError};

/// The virtual screen the demo composes into; a host integer-upscales it.
pub const VIRTUAL: Size = Size { w: 640, h: 360 };
const BACKDROP: Color = Color::rgb(0x10, 0x12, 0x28);
/// Source-pixel height the banners rasterise at.
const CELL_PX: u32 = 32;

/// The compositor for the demo's fixed virtual screen.
#[must_use]
pub fn presentation() -> Presentation {
    Presentation::new(VIRTUAL, BACKDROP, Color::rgb(0, 0, 0), 1)
}

/// The demo: the prompt card, chained into the win card on Enter. The banners
/// bake once through `font` (the source is not kept).
///
/// # Errors
/// [`DemoError::Font`] if `font` cannot be loaded.
pub fn build(font: &FontConfig) -> Result<PromptScreen<DemoCtx>, DemoError> {
    let source = RasterGlyphSource::new(SystemFont::load(font)?, CELL_PX);
    let factory = ShadowBannerFactory::new(&source, ShadowStyle::default(), VIRTUAL);
    let prompt = factory.centered("PRESS ENTER", 1);
    let win = factory.centered("YOU WIN!", 1);

    // The win card: any exit quits (a terminal card holds nothing further).
    let win_screen = PromptScreen::new(vec![win], |exit, ctx: &mut DemoCtx| {
        match exit {
            PromptExit::Confirmed | PromptExit::Cancelled => ctx.quit = true,
            PromptExit::Idled => {}
        }
        ScreenChange::None
    });
    // The prompt card: Enter reveals the win card, Esc quits.
    Ok(PromptScreen::new(
        vec![prompt],
        |exit, ctx: &mut DemoCtx| match exit {
            PromptExit::Confirmed => ScreenChange::Replace(Box::new(win_screen)),
            PromptExit::Cancelled => {
                ctx.quit = true;
                ScreenChange::None
            }
            PromptExit::Idled => ScreenChange::None,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_deterministically_from_the_embedded_face() {
        let font = FontConfig::default().with_embedded_font();
        assert!(build(&font).is_ok(), "the embedded face always loads");
    }
}

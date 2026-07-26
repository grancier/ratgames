//! `high_score` — show a ranked high-score board until Enter/Esc.
//!
//! A showcase of the reusable high-score stack: a [`HighScores`] table (loaded
//! by the caller through the [`HighScoreStore`](ratgames::HighScoreStore) seam
//! — the native example uses the JSON file backend, the web build an in-memory
//! store seeded with the same data) is baked by [`HighScoreBoard`] — header,
//! ranked rows, footer — into pixel-art banners through a
//! [`ShadowBannerFactory`], and a [`PromptScreen`] holds it until Enter/Esc.

use ratgames::{
    BoardFooter, BoardLine, Color, FontConfig, HighScoreBoard, HighScoreBoardSpec, HighScoreLayout,
    HighScores, Point, Presentation, PromptExit, PromptScreen, RasterGlyphSource, ScreenChange,
    ShadowBannerFactory, ShadowStyle, Size, SystemFont,
};

use crate::{DemoCtx, DemoError};

/// The virtual screen the demo composes into; a host integer-upscales it.
pub const VIRTUAL: Size = Size { w: 640, h: 360 };
const BACKDROP: Color = Color::rgb(0x10, 0x12, 0x28);
/// Source-pixel height the board's rows rasterise at — the 32px body-text
/// standard the games share.
const CELL_PX: u32 = 32;

/// The compositor for the demo's fixed virtual screen.
#[must_use]
pub fn presentation() -> Presentation {
    Presentation::new(VIRTUAL, BACKDROP, Color::rgb(0, 0, 0), 1)
}

/// The demo: `scores` baked into the ranked board, held on a card until the
/// player confirms or cancels (either way, the demo quits). The banners bake
/// once through `font` (the source is not kept).
///
/// # Errors
/// [`DemoError::Font`] if `font` cannot be loaded.
pub fn build(scores: &HighScores, font: &FontConfig) -> Result<PromptScreen<DemoCtx>, DemoError> {
    let source = RasterGlyphSource::new(SystemFont::load(font)?, CELL_PX);
    let factory = ShadowBannerFactory::new(&source, ShadowStyle::default(), VIRTUAL);
    let banners = HighScoreBoard::new(
        scores,
        &factory,
        HighScoreBoardSpec {
            layout: HighScoreLayout {
                origin: Point::new(176, 92),
                row_pitch: 42,
                column_width: 500,
                rows_per_column: 8,
                name_width: 6,
            },
            capacity: 5,
            row_scale: 1,
            header: Some(BoardLine {
                text: "HIGH SCORES",
                at: Point::new(176, 24),
                scale: 1,
            }),
            footer: Some(BoardFooter {
                text: "PRESS ENTER",
                gap_below_rows: 18,
                scale: 1,
            }),
        },
    )
    .into_banners();

    // Hold the board until the player confirms or cancels; either way, quit.
    Ok(PromptScreen::new(banners, |exit, ctx: &mut DemoCtx| {
        match exit {
            PromptExit::Confirmed | PromptExit::Cancelled => ctx.quit = true,
            PromptExit::Idled => {}
        }
        ScreenChange::None
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_deterministically_from_the_embedded_face() {
        let mut scores = HighScores::new();
        scores.record("ADA", 300, 8);
        scores.record("GRACE", 100, 8);
        let font = FontConfig::default().with_embedded_font();
        assert!(
            build(&scores, &font).is_ok(),
            "the embedded face always loads"
        );
    }
}

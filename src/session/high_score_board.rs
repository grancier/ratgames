//! [`HighScoreBoard`] — a [`HighScores`] board baked into pixel-art banners.
//!
//! The rendering half of a high-score screen, the complement to the pure
//! [`HighScoreLayout`](super::HighScoreLayout): given a board, a layout, and a
//! game's [`ShadowBannerFactory`], it bakes the ranked rows — and an optional
//! header and footer — into a stack of [`ShadowBanner`]s the game drops into its
//! own screen. The layout owns *where* rows sit; this owns turning them (and the
//! game's header/footer copy) into banners.
//!
//! It is deliberately neither a `Screen` nor an [`OverlayLayer`]: the board renders
//! scores, and the game decides what wraps it — the header/footer *copy*, the text
//! *style*, and what `Enter` / `Esc` mean (reset to a title, quit, …) are the
//! game's, passed in via [`HighScoreBoardSpec`] or kept in the game's screen. The
//! board just [`collect_layers`](HighScoreBoard::collect_layers) its banners into
//! the frame.

use super::{HighScoreLayout, HighScores};
use crate::geometry::Point;
use crate::present::OverlayLayer;
use crate::ui::{ShadowBanner, ShadowBannerFactory};

/// A single fixed board line (a header or any one-off caption): its text, where it
/// anchors in virtual-screen pixels, and its magnification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardLine<'a> {
    /// The line's text (game copy, e.g. `"HIGH SCORES"`).
    pub text: &'a str,
    /// Where to anchor it, in virtual-screen pixels.
    pub at: Point,
    /// Magnification for this line.
    pub scale: u32,
}

/// A footer placed under the board's tallest column: its text, the gap below the
/// rows (in virtual-screen pixels), and its magnification. Its anchor is derived
/// from the layout's [`below`](HighScoreLayout::below), so it tracks the row count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardFooter<'a> {
    /// The footer's text (game copy, e.g. `"PRESS ENTER"`).
    pub text: &'a str,
    /// Gap below the tallest column, in virtual-screen pixels.
    pub gap_below_rows: i32,
    /// Magnification for the footer.
    pub scale: u32,
}

/// How a [`HighScoreBoard`] bakes: the row [`HighScoreLayout`], how many entries to
/// show, the row magnification, and optional header / footer lines. The game's copy
/// and style values live here; the reusable baking lives in [`HighScoreBoard`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighScoreBoardSpec<'a> {
    /// Where and how the ranked rows are placed.
    pub layout: HighScoreLayout,
    /// Show at most this many entries.
    pub capacity: usize,
    /// Magnification for the ranked rows.
    pub row_scale: u32,
    /// An optional header line (a title), anchored at its own position.
    pub header: Option<BoardLine<'a>>,
    /// An optional footer line, anchored under the rows.
    pub footer: Option<BoardFooter<'a>>,
}

/// A [`HighScores`] board baked into pixel-art banners: an optional header, the
/// ranked rows, then an optional footer, in draw order. Build with
/// [`new`](Self::new) and hand the banners to the frame with
/// [`collect_layers`](Self::collect_layers).
pub struct HighScoreBoard {
    banners: Vec<ShadowBanner>,
}

impl HighScoreBoard {
    /// Bake `scores` into banners per `spec`, through `factory`: the header (if
    /// any), then the ranked rows (up to `spec.capacity`, placed by
    /// `spec.layout`), then the footer (if any, anchored below the rows).
    #[must_use]
    pub fn new(
        scores: &HighScores,
        factory: &ShadowBannerFactory<'_>,
        spec: HighScoreBoardSpec<'_>,
    ) -> Self {
        let mut banners = Vec::new();

        if let Some(header) = spec.header {
            banners.push(factory.at(header.text, header.at, header.scale));
        }

        for row in spec.layout.rows(scores, spec.capacity) {
            banners.push(factory.at(&row.text, row.at, spec.row_scale));
        }

        if let Some(footer) = spec.footer {
            let below = spec.layout.below(scores, spec.capacity);
            let at = Point::new(below.x, below.y + footer.gap_below_rows);
            banners.push(factory.at(footer.text, at, footer.scale));
        }

        Self { banners }
    }

    /// Contribute the board's banners to the frame's overlays, in draw order.
    pub fn collect_layers<'a>(&'a self, overlays: &mut Vec<&'a dyn OverlayLayer>) {
        for banner in &self.banners {
            overlays.push(banner);
        }
    }

    /// The baked banners themselves, in draw order — for a host that takes
    /// ownership (an attract-mode [`TimedCard`](super::TimedCard) showing the
    /// board) rather than borrowing them each frame.
    #[must_use]
    pub fn into_banners(self) -> Vec<ShadowBanner> {
        self.banners
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Size;
    use crate::glyph::Bitmap8x8;
    use crate::ui::{ShadowBannerFactory, ShadowStyle};

    /// A board of `n` distinct entries (their content is irrelevant to baking).
    fn board_of(n: usize) -> HighScores {
        let mut b = HighScores::new();
        for i in 0..n {
            b.record(format!("P{i}"), (n - i) as u32 * 10, 100);
        }
        b
    }

    fn layout() -> HighScoreLayout {
        HighScoreLayout {
            origin: Point::new(16, 60),
            row_pitch: 36,
            column_width: 300,
            rows_per_column: 5,
            name_width: 5,
        }
    }

    fn header() -> BoardLine<'static> {
        BoardLine {
            text: "HIGH SCORES",
            at: Point::new(16, 8),
            scale: 2,
        }
    }

    fn footer() -> BoardFooter<'static> {
        BoardFooter {
            text: "PRESS ENTER",
            gap_below_rows: 12,
            scale: 1,
        }
    }

    /// Bake a board and count the banners it contributes to a frame.
    fn banner_count(scores: &HighScores, spec: HighScoreBoardSpec<'_>) -> usize {
        let src = Bitmap8x8;
        let factory = ShadowBannerFactory::new(&src, ShadowStyle::default(), Size::new(256, 256));
        let board = HighScoreBoard::new(scores, &factory, spec);
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        board.collect_layers(&mut overlays);
        overlays.len()
    }

    #[test]
    fn bakes_a_header_every_row_and_a_footer() {
        let scores = board_of(3);
        let count = banner_count(
            &scores,
            HighScoreBoardSpec {
                layout: layout(),
                capacity: 10,
                row_scale: 1,
                header: Some(header()),
                footer: Some(footer()),
            },
        );
        assert_eq!(count, 1 + 3 + 1);
    }

    #[test]
    fn omits_an_absent_header_and_footer() {
        let scores = board_of(2);
        let count = banner_count(
            &scores,
            HighScoreBoardSpec {
                layout: layout(),
                capacity: 10,
                row_scale: 1,
                header: None,
                footer: None,
            },
        );
        assert_eq!(count, 2, "just the rows");
    }

    #[test]
    fn caps_the_rows_at_capacity() {
        let scores = board_of(8);
        let count = banner_count(
            &scores,
            HighScoreBoardSpec {
                layout: layout(),
                capacity: 3,
                row_scale: 1,
                header: None,
                footer: None,
            },
        );
        assert_eq!(count, 3);
    }

    #[test]
    fn into_banners_yields_the_same_stack_collect_layers_borrows() {
        let scores = board_of(3);
        let src = Bitmap8x8;
        let factory = ShadowBannerFactory::new(&src, ShadowStyle::default(), Size::new(256, 256));
        let spec = HighScoreBoardSpec {
            layout: layout(),
            capacity: 10,
            row_scale: 1,
            header: Some(header()),
            footer: Some(footer()),
        };
        let banners = HighScoreBoard::new(&scores, &factory, spec).into_banners();
        assert_eq!(banners.len(), 1 + 3 + 1);
    }

    #[test]
    fn an_empty_board_still_bakes_the_header_and_footer() {
        let scores = HighScores::new();
        let count = banner_count(
            &scores,
            HighScoreBoardSpec {
                layout: layout(),
                capacity: 10,
                row_scale: 1,
                header: Some(header()),
                footer: Some(footer()),
            },
        );
        assert_eq!(count, 2, "header and footer, no rows");
    }
}

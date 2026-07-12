//! [`BannerColumn`] — a bake plan for a column of banner lines: a shared left
//! x, per-line y and scale, baked into `Vec<ShadowBanner>` through the game's
//! factory.
//!
//! Not a screen. The how-to card, a level intro, and a level-clear tally all
//! bake "configured texts down a layout Ys vector at a shared margin", each
//! hand-rolling the same zip-and-guard closure. The column is that plan:
//! [`line`](BannerColumn::line) places one text at an explicit y (a title over
//! the body, typically at its own scale), [`lines`](BannerColumn::lines) zips
//! texts against a Ys vector, and [`bake`](BannerColumn::bake) renders the lot
//! for a [`TimedCard`](crate::TimedCard) / [`AttractCard`](crate::AttractCard)
//! / [`PromptScreen`](crate::PromptScreen) to own. The texts arrive already
//! filled (copy is the game's) and the x / Ys / scales are the game's layout
//! values — the column owns only the placement plan.

use super::shadow_banner::{ShadowBanner, ShadowBannerFactory};
use crate::geometry::Point;

/// One planned line: its (already-filled) text, vertical position, and
/// magnification.
struct ColumnLine {
    text: String,
    y: i32,
    scale: u32,
}

/// A left-anchored column of banner lines, built up with
/// [`line`](Self::line) / [`lines`](Self::lines) and rendered with
/// [`bake`](Self::bake).
pub struct BannerColumn {
    x: i32,
    lines: Vec<ColumnLine>,
}

impl BannerColumn {
    /// A column whose every line starts at `x` — the game's shared margin.
    #[must_use]
    pub fn at_x(x: i32) -> Self {
        Self {
            x,
            lines: Vec::new(),
        }
    }

    /// Plan one line at an explicit `y` — a title over the body lines,
    /// typically at its own scale.
    #[must_use]
    pub fn line(mut self, text: impl Into<String>, y: i32, scale: u32) -> Self {
        self.lines.push(ColumnLine {
            text: text.into(),
            y,
            scale,
        });
        self
    }

    /// Plan body lines zipped against a layout Ys vector, all at `scale`. A
    /// text past the end of `ys` lands at `0` — visibly wrong rather than a
    /// panic, because the Ys are authored data a game live-tunes.
    #[must_use]
    pub fn lines<I>(mut self, texts: I, ys: &[i32], scale: u32) -> Self
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        for (i, text) in texts.into_iter().enumerate() {
            let y = ys.get(i).copied().unwrap_or(0);
            self.lines.push(ColumnLine {
                text: text.into(),
                y,
                scale,
            });
        }
        self
    }

    /// Bake the planned lines through the game's factory, in plan order.
    #[must_use]
    pub fn bake(&self, factory: &ShadowBannerFactory) -> Vec<ShadowBanner> {
        self.lines
            .iter()
            .map(|line| factory.at(&line.text, Point::new(self.x, line.y), line.scale))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Size;
    use crate::glyph::Bitmap8x8;
    use crate::ui::ShadowStyle;

    #[test]
    fn plans_a_title_and_body_lines_in_order() {
        let column = BannerColumn::at_x(40)
            .line("TITLE", 10, 2)
            .lines(["a", "b"], &[20, 30], 1);
        let planned: Vec<(&str, i32, u32)> = column
            .lines
            .iter()
            .map(|line| (line.text.as_str(), line.y, line.scale))
            .collect();
        assert_eq!(planned, [("TITLE", 10, 2), ("a", 20, 1), ("b", 30, 1)]);
    }

    #[test]
    fn a_text_past_the_ys_vector_lands_at_zero() {
        let column = BannerColumn::at_x(0).lines(["a", "b", "c"], &[20, 30], 1);
        assert_eq!(
            column.lines[2].y, 0,
            "missing y is visibly wrong, not a panic"
        );
    }

    #[test]
    fn bake_renders_every_planned_line() {
        let factory =
            ShadowBannerFactory::new(&Bitmap8x8, ShadowStyle::default(), Size::new(64, 64));
        let banners = BannerColumn::at_x(4)
            .line("T", 2, 1)
            .lines(["x", "y", "z"], &[10, 20, 30], 1)
            .bake(&factory);
        assert_eq!(banners.len(), 4);
    }
}

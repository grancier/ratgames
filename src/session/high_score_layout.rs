//! Laying a [`HighScores`] board out as ranked text rows for rendering.
//!
//! [`HighScoreLayout`] is the pure, font-free half of a high-score screen: it
//! turns a board into positioned, formatted rows — "rank name points", placed
//! top-to-bottom and wrapped into columns — and leaves the glyph rendering, the
//! header/footer copy, and the screen transition to the game. Ten rows at a
//! title-sized font overflow a short screen, so the column wrap is the fiddly bit
//! worth getting right (and testing) once, apart from any font.

use super::HighScores;
use crate::geometry::Point;

/// One formatted, positioned board row: the text to draw and where (in
/// virtual-screen pixels) to anchor it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedRow {
    /// The rendered row text, e.g. `" 1 ADA    300"`.
    pub text: String,
    /// Where to anchor the row, in virtual-screen pixels.
    pub at: Point,
}

/// How a [`HighScores`] board lays out as ranked rows: where the first row sits,
/// the row and column spacing, how many rows fill a column before wrapping, and
/// the name field width. All in virtual-screen pixels; the game renders each row
/// in whatever style it likes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighScoreLayout {
    /// Top-left of the first (rank 1) row.
    pub origin: Point,
    /// Vertical distance between rows within a column.
    pub row_pitch: i32,
    /// Horizontal distance between columns.
    pub column_width: i32,
    /// Rows filling a column before the next entry wraps to the next column.
    /// Treated as at least 1 (a zero would leave nowhere to place a row).
    pub rows_per_column: usize,
    /// Name field width: names are upper-cased and clipped to it, and padded to it
    /// so the points column stays aligned.
    pub name_width: usize,
}

impl HighScoreLayout {
    /// The board's entries (up to `capacity`) as ranked, positioned rows —
    /// `"{rank:>2} {NAME:<name_width}{points:>5}"` — filling a column
    /// top-to-bottom, then wrapping to the next after `rows_per_column`.
    #[must_use]
    pub fn rows(&self, board: &HighScores, capacity: usize) -> Vec<PlacedRow> {
        let per_column = self.rows_per_column.max(1);
        board
            .entries()
            .iter()
            .take(capacity)
            .enumerate()
            .map(|(i, entry)| {
                let name: String = entry
                    .name
                    .to_uppercase()
                    .chars()
                    .take(self.name_width)
                    .collect();
                let text = format!(
                    "{:>2} {:<width$}{:>5}",
                    i + 1,
                    name,
                    entry.points,
                    width = self.name_width
                );
                let column = (i / per_column) as i32;
                let row = (i % per_column) as i32;
                let at = Point::new(
                    self.origin.x + column * self.column_width,
                    self.origin.y + row * self.row_pitch,
                );
                PlacedRow { text, at }
            })
            .collect()
    }

    /// The anchor just past the bottom of the tallest column — where a game can
    /// place a footer under the board. The first column fills first, so it is the
    /// tallest; the game adds its own gap below this point.
    #[must_use]
    pub fn below(&self, board: &HighScores, capacity: usize) -> Point {
        let shown = board.entries().len().min(capacity);
        let rows_deep = shown.min(self.rows_per_column.max(1)) as i32;
        Point::new(self.origin.x, self.origin.y + rows_deep * self.row_pitch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(pairs: &[(&str, u32)]) -> HighScores {
        let mut b = HighScores::new();
        for (name, points) in pairs {
            b.record(*name, *points, 100);
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

    #[test]
    fn a_row_is_rank_name_points_at_the_origin() {
        let rows = layout().rows(&board(&[("ada", 300)]), 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, format!("{:>2} {:<5}{:>5}", 1, "ADA", 300));
        assert_eq!(rows[0].at, Point::new(16, 60));
    }

    #[test]
    fn names_are_upper_cased_and_clipped_to_width() {
        let rows = layout().rows(&board(&[("Lovelace", 1)]), 10);
        assert!(rows[0].text.contains("LOVEL")); // upper-cased, clipped to 5
        assert!(!rows[0].text.contains("LOVELA")); // the 6th char is dropped
    }

    #[test]
    fn entries_wrap_top_to_bottom_then_into_columns() {
        // 7 entries, 5 per column: rows 0..=4 fill column 0, then 5..=6 start column 1.
        let seven = board(&[
            ("A", 70),
            ("B", 60),
            ("C", 50),
            ("D", 40),
            ("E", 30),
            ("F", 20),
            ("G", 10),
        ]);
        let rows = layout().rows(&seven, 10);
        assert_eq!(rows.len(), 7);
        assert_eq!(rows[0].at, Point::new(16, 60)); // column 0, row 0
        assert_eq!(rows[4].at, Point::new(16, 60 + 4 * 36)); // column 0, row 4
        assert_eq!(rows[5].at, Point::new(16 + 300, 60)); // column 1, row 0
        assert_eq!(rows[6].at, Point::new(16 + 300, 60 + 36)); // column 1, row 1
    }

    #[test]
    fn capacity_caps_the_displayed_rows() {
        let eight = board(&[
            ("A", 8),
            ("B", 7),
            ("C", 6),
            ("D", 5),
            ("E", 4),
            ("F", 3),
            ("G", 2),
            ("H", 1),
        ]);
        assert_eq!(layout().rows(&eight, 3).len(), 3);
    }

    #[test]
    fn below_sits_under_the_tallest_column() {
        // Three entries fill one column three deep.
        let three = board(&[("A", 3), ("B", 2), ("C", 1)]);
        assert_eq!(layout().below(&three, 10), Point::new(16, 60 + 3 * 36));
        // Seven entries fill the first column (five), which is the tallest.
        let seven = board(&[
            ("A", 7),
            ("B", 6),
            ("C", 5),
            ("D", 4),
            ("E", 3),
            ("F", 2),
            ("G", 1),
        ]);
        assert_eq!(layout().below(&seven, 10), Point::new(16, 60 + 5 * 36));
    }

    #[test]
    fn an_empty_board_has_no_rows_and_a_footer_at_the_origin() {
        let empty = HighScores::new();
        assert!(layout().rows(&empty, 10).is_empty());
        assert_eq!(layout().below(&empty, 10), Point::new(16, 60));
    }
}

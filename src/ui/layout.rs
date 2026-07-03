//! Splitting a rect into child rects by constraints — the positioning primitive
//! that complements [`Panel::content_rect`](super::Panel::content_rect).
//!
//! A minimal take on Ratatui's `Layout`: pick an [`Axis`], give a list of
//! [`Constraint`]s, and [`split`] returns one rect per constraint along that axis
//! (each spanning the full cross-axis extent). Fixed and proportional
//! constraints resolve first; [`Constraint::Fill`] shares whatever is left.

use crate::geometry::{Point, Rect, Size};

/// The axis a [`split`] runs along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Lay children left to right; constraint sizes are widths.
    Horizontal,
    /// Lay children top to bottom; constraint sizes are heights.
    Vertical,
}

/// How much of the axis a child claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constraint {
    /// A fixed number of pixels.
    Length(u32),
    /// A percentage (`0..=100`) of the whole axis.
    Percentage(u32),
    /// A fraction `num/den` of the whole axis.
    Ratio(u32, u32),
    /// A share of the space left after the fixed/proportional constraints, split
    /// between all `Fill`s by weight.
    Fill(u32),
}

impl Constraint {
    /// The resolved pixel size on a `total`-pixel axis. `Fill` resolves to `0`
    /// here — it is filled from the remainder in [`split`].
    fn resolved(self, total: u32) -> u32 {
        let total = u64::from(total);
        let size = match self {
            Constraint::Length(n) => u64::from(n),
            Constraint::Percentage(p) => total * u64::from(p.min(100)) / 100,
            Constraint::Ratio(a, b) if b != 0 => total * u64::from(a) / u64::from(b),
            Constraint::Ratio(..) | Constraint::Fill(_) => 0,
        };
        size.min(total) as u32
    }
}

/// Divide `area` along `axis`, one rect per constraint. Non-`Fill` constraints
/// take their resolved size; the remaining space is split between the `Fill`s by
/// weight (the last `Fill` absorbs any rounding remainder). Sizes are clamped so
/// the run never spills past `area`.
#[must_use]
pub fn split(area: Rect, axis: Axis, constraints: &[Constraint]) -> Vec<Rect> {
    let total = match axis {
        Axis::Horizontal => area.size.w,
        Axis::Vertical => area.size.h,
    };

    let mut sizes: Vec<u32> = constraints.iter().map(|c| c.resolved(total)).collect();
    let remaining = total.saturating_sub(sizes.iter().sum());
    let fill_weight: u32 = constraints
        .iter()
        .map(|c| if let Constraint::Fill(w) = c { *w } else { 0 })
        .sum();
    if fill_weight > 0 {
        let last_fill = constraints
            .iter()
            .rposition(|c| matches!(c, Constraint::Fill(_)));
        let mut given = 0u32;
        for (i, c) in constraints.iter().enumerate() {
            if let Constraint::Fill(w) = *c {
                let share = if Some(i) == last_fill {
                    remaining - given
                } else {
                    (u64::from(remaining) * u64::from(w) / u64::from(fill_weight)) as u32
                };
                given += share;
                sizes[i] = share;
            }
        }
    }

    let mut rects = Vec::with_capacity(sizes.len());
    let mut x = area.origin.x;
    let mut y = area.origin.y;
    for size in sizes {
        let rect = match axis {
            Axis::Horizontal => {
                let w = (size as i32).min((area.right() - x).max(0)) as u32;
                let r = Rect::new(Point::new(x, y), Size::new(w, area.size.h));
                x += w as i32;
                r
            }
            Axis::Vertical => {
                let h = (size as i32).min((area.bottom() - y).max(0)) as u32;
                let r = Rect::new(Point::new(x, y), Size::new(area.size.w, h));
                y += h as i32;
                r
            }
        };
        rects.push(rect);
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_then_fill_takes_the_remainder() {
        let area = Rect::from_size(Size::new(100, 10));
        let r = split(
            area,
            Axis::Horizontal,
            &[Constraint::Length(30), Constraint::Fill(1)],
        );
        assert_eq!(r[0], Rect::new(Point::new(0, 0), Size::new(30, 10)));
        assert_eq!(r[1], Rect::new(Point::new(30, 0), Size::new(70, 10)));
    }

    #[test]
    fn two_fills_share_the_space_by_weight() {
        let area = Rect::from_size(Size::new(100, 10));
        let r = split(
            area,
            Axis::Horizontal,
            &[Constraint::Fill(1), Constraint::Fill(3)],
        );
        assert_eq!(r[0].size.w, 25);
        assert_eq!(r[1].size.w, 75); // last fill absorbs the rest
    }

    #[test]
    fn percentage_and_ratio_are_fractions_of_the_axis() {
        let area = Rect::from_size(Size::new(200, 50));
        let r = split(
            area,
            Axis::Vertical,
            &[Constraint::Percentage(25), Constraint::Ratio(1, 2)],
        );
        assert_eq!(r[0].size.h, 12); // 50 * 25 / 100
        assert_eq!(r[1].size.h, 25); // 50 * 1 / 2
        assert_eq!(r[0].size.w, 200); // cross axis spans the area
        assert_eq!(r[1].origin.y, 12);
    }

    #[test]
    fn oversubscribed_lengths_clamp_to_the_area() {
        let area = Rect::from_size(Size::new(50, 10));
        let r = split(
            area,
            Axis::Horizontal,
            &[Constraint::Length(40), Constraint::Length(40)],
        );
        assert_eq!(r[0].size.w, 40);
        assert_eq!(r[1].size.w, 10); // only 10 px remain
    }
}

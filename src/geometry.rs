//! Integer geometry primitives shared across surfaces, sprites, and layers.
//!
//! Sizes are unsigned (a buffer cannot have negative extent); positions are
//! signed so a sprite can be placed partially off the top-left edge and clip.

/// Width/height in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

impl Size {
    #[must_use]
    pub const fn new(w: u32, h: u32) -> Self {
        Self { w, h }
    }

    /// Number of pixels — the length a backing buffer must have.
    #[must_use]
    pub const fn area(self) -> usize {
        self.w as usize * self.h as usize
    }

    /// Whether `p` falls inside `0..w × 0..h`.
    #[must_use]
    pub fn contains(self, p: Point) -> bool {
        p.x >= 0 && p.y >= 0 && (p.x as i64) < i64::from(self.w) && (p.y as i64) < i64::from(self.h)
    }
}

/// A signed pixel coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const ORIGIN: Point = Point { x: 0, y: 0 };

    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// An axis-aligned rectangle: top-left `origin` plus `size`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    #[must_use]
    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    /// A rect at the origin with the given size.
    #[must_use]
    pub const fn from_size(size: Size) -> Self {
        Self {
            origin: Point::ORIGIN,
            size,
        }
    }

    #[must_use]
    pub const fn right(self) -> i32 {
        self.origin.x + self.size.w as i32
    }

    #[must_use]
    pub const fn bottom(self) -> i32 {
        self.origin.y + self.size.h as i32
    }

    #[must_use]
    pub fn contains(self, p: Point) -> bool {
        p.x >= self.origin.x && p.y >= self.origin.y && p.x < self.right() && p.y < self.bottom()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_is_width_times_height() {
        assert_eq!(Size::new(256, 240).area(), 61_440);
    }

    #[test]
    fn size_contains_only_the_half_open_box() {
        let s = Size::new(4, 3);
        assert!(s.contains(Point::new(0, 0)));
        assert!(s.contains(Point::new(3, 2)));
        assert!(!s.contains(Point::new(4, 0)));
        assert!(!s.contains(Point::new(0, 3)));
        assert!(!s.contains(Point::new(-1, 0)));
    }

    #[test]
    fn rect_contains_respects_offset_origin() {
        let r = Rect::new(Point::new(10, 20), Size::new(5, 5));
        assert!(r.contains(Point::new(10, 20)));
        assert!(r.contains(Point::new(14, 24)));
        assert!(!r.contains(Point::new(15, 20)));
        assert!(!r.contains(Point::new(9, 20)));
    }
}

//! [`Sprite`] — the reusable unit of pixel art.
//!
//! A fixed-size grid of [`Color`] with 1-bit transparency. Glyphs, tiles,
//! characters, and the marquee banner are all just sprites; anything that draws
//! 8-bit art produces or consumes one, so there is a single blit path.

use crate::color::Color;
use crate::geometry::{Point, Size};

/// A rectangular grid of colours. Transparent cells are skipped when blitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    size: Size,
    pixels: Vec<Color>,
}

impl Sprite {
    /// A fully transparent sprite of the given size.
    #[must_use]
    pub fn new(size: Size) -> Self {
        Self {
            size,
            pixels: vec![Color::TRANSPARENT; size.area()],
        }
    }

    /// Build from a row-major pixel buffer.
    ///
    /// # Errors
    /// Returns [`SpriteError::SizeMismatch`] if `pixels.len() != size.area()`.
    pub fn from_pixels(size: Size, pixels: Vec<Color>) -> Result<Self, SpriteError> {
        if pixels.len() == size.area() {
            Ok(Self { size, pixels })
        } else {
            Err(SpriteError::SizeMismatch {
                expected: size.area(),
                got: pixels.len(),
            })
        }
    }

    #[must_use]
    pub fn size(&self) -> Size {
        self.size
    }

    /// The colour at `p`, or [`Color::TRANSPARENT`] outside the grid.
    #[must_use]
    pub fn get(&self, p: Point) -> Color {
        if self.size.contains(p) {
            self.pixels[self.index(p)]
        } else {
            Color::TRANSPARENT
        }
    }

    /// Set the colour at `p`; a no-op outside the grid.
    pub fn set(&mut self, p: Point, color: Color) {
        if self.size.contains(p) {
            let i = self.index(p);
            self.pixels[i] = color;
        }
    }

    fn index(&self, p: Point) -> usize {
        p.y as usize * self.size.w as usize + p.x as usize
    }
}

/// Errors constructing a [`Sprite`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SpriteError {
    /// The pixel buffer length did not match the declared size.
    #[error("pixel buffer length {got} does not match {expected} for the given size")]
    SizeMismatch { expected: usize, got: usize },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sprite_is_all_transparent() {
        let s = Sprite::new(Size::new(3, 2));
        assert_eq!(s.size(), Size::new(3, 2));
        assert_eq!(s.get(Point::new(1, 1)), Color::TRANSPARENT);
    }

    #[test]
    fn from_pixels_validates_length() {
        let ok = Sprite::from_pixels(Size::new(2, 1), vec![Color::TRANSPARENT; 2]);
        assert!(ok.is_ok());

        let err = Sprite::from_pixels(Size::new(2, 2), vec![Color::TRANSPARENT; 3]);
        assert_eq!(
            err.unwrap_err(),
            SpriteError::SizeMismatch {
                expected: 4,
                got: 3
            }
        );
    }

    #[test]
    fn get_set_roundtrip_and_out_of_bounds() {
        let mut s = Sprite::new(Size::new(2, 2));
        let red = Color::rgb(255, 0, 0);
        s.set(Point::new(1, 0), red);
        assert_eq!(s.get(Point::new(1, 0)), red);
        // Out of bounds set is a no-op; out of bounds get is transparent.
        s.set(Point::new(5, 5), red);
        assert_eq!(s.get(Point::new(5, 5)), Color::TRANSPARENT);
    }
}

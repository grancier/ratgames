//! [`Surface`] — an owned, blittable pixel buffer.
//!
//! One type backs both the low-resolution virtual screen and the physical
//! window framebuffer, so compositing, sprite blitting, and the integer upscale
//! all live here (DRY) rather than being re-derived per call site.

use crate::color::Color;
use crate::geometry::{Point, Size};
use crate::sprite::Sprite;

/// A rectangular buffer of `0x00RRGGBB` words (framebuffer-ready).
#[derive(Debug, Clone)]
pub struct Surface {
    size: Size,
    buf: Vec<u32>,
}

impl Surface {
    /// A surface filled with `fill`.
    #[must_use]
    pub fn new(size: Size, fill: Color) -> Self {
        Self {
            size,
            buf: vec![fill.packed(); size.area()],
        }
    }

    #[must_use]
    pub fn size(&self) -> Size {
        self.size
    }

    /// The raw buffer, for handing to the window backend.
    #[must_use]
    pub fn as_slice(&self) -> &[u32] {
        &self.buf
    }

    /// Overwrite every pixel with `color`.
    pub fn fill(&mut self, color: Color) {
        self.buf.fill(color.packed());
    }

    /// Write one pixel, skipping transparent colours and out-of-bounds points.
    pub fn set(&mut self, p: Point, color: Color) {
        if color.is_visible() && self.size.contains(p) {
            let i = self.index(p);
            self.buf[i] = color.packed();
        }
    }

    /// Composite `sprite` with its top-left at `at`, honouring transparency and
    /// clipping to the surface bounds.
    pub fn draw_sprite(&mut self, sprite: &Sprite, at: Point) {
        let s = sprite.size();
        for sy in 0..s.h {
            for sx in 0..s.w {
                let src = Point::new(sx as i32, sy as i32);
                self.set(Point::new(at.x + src.x, at.y + src.y), sprite.get(src));
            }
        }
    }

    /// Nearest-neighbour integer upscale of `src` into this surface: every
    /// source pixel becomes a `scale × scale` block whose top-left lands at
    /// `dst`. Clipped to bounds. `scale` is treated as at least 1.
    ///
    /// `src` is opaque (a screen, not a sprite), so its pixels are copied
    /// verbatim without a transparency test — the hot path of the frame.
    pub fn draw_upscaled(&mut self, src: &Surface, scale: u32, dst: Point) {
        let scale = scale.max(1);
        let s = src.size;
        for sy in 0..s.h {
            for sx in 0..s.w {
                let word = src.buf[sy as usize * s.w as usize + sx as usize];
                let base_x = dst.x + (sx * scale) as i32;
                let base_y = dst.y + (sy * scale) as i32;
                for ry in 0..scale {
                    let y = base_y + ry as i32;
                    if y < 0 || y >= self.size.h as i32 {
                        continue;
                    }
                    for rx in 0..scale {
                        let x = base_x + rx as i32;
                        if x < 0 || x >= self.size.w as i32 {
                            continue;
                        }
                        self.buf[y as usize * self.size.w as usize + x as usize] = word;
                    }
                }
            }
        }
    }

    fn index(&self, p: Point) -> usize {
        p.y as usize * self.size.w as usize + p.x as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word_at(s: &Surface, x: u32, y: u32) -> u32 {
        s.as_slice()[y as usize * s.size().w as usize + x as usize]
    }

    #[test]
    fn fill_and_set_and_transparency() {
        let bg = Color::rgb(0, 0, 0);
        let mut s = Surface::new(Size::new(2, 2), bg);
        assert_eq!(word_at(&s, 0, 0), bg.packed());

        let red = Color::rgb(255, 0, 0);
        s.set(Point::new(1, 1), red);
        assert_eq!(word_at(&s, 1, 1), red.packed());

        // Transparent write is ignored; so is out of bounds.
        s.set(Point::new(0, 0), Color::TRANSPARENT);
        assert_eq!(word_at(&s, 0, 0), bg.packed());
        s.set(Point::new(9, 9), red); // must not panic
    }

    #[test]
    fn draw_sprite_composites_and_clips() {
        let mut s = Surface::new(Size::new(3, 3), Color::rgb(0, 0, 0));
        let mut spr = Sprite::new(Size::new(2, 2));
        let red = Color::rgb(255, 0, 0);
        spr.set(Point::new(0, 0), red); // rest transparent
                                        // Place straddling the bottom-right corner: only (2,2) lands.
        s.draw_sprite(&spr, Point::new(2, 2));
        assert_eq!(word_at(&s, 2, 2), red.packed());
        assert_eq!(word_at(&s, 0, 0), Color::rgb(0, 0, 0).packed());
    }

    #[test]
    fn draw_upscaled_expands_each_pixel_to_a_block() {
        let mut src = Surface::new(Size::new(2, 1), Color::rgb(0, 0, 0));
        let red = Color::rgb(255, 0, 0);
        src.set(Point::new(0, 0), red);

        let mut dst = Surface::new(Size::new(4, 2), Color::rgb(0, 0, 0));
        dst.draw_upscaled(&src, 2, Point::ORIGIN);

        // The red source pixel becomes the top-left 2x2 block.
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(word_at(&dst, x, y), red.packed());
        }
        // The transparent-black source pixel becomes the next 2x2 block.
        assert_eq!(word_at(&dst, 2, 0), Color::rgb(0, 0, 0).packed());
    }
}

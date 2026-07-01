//! [`Marquee`] — scrolls a sprite strip horizontally, wrapping seamlessly.
//!
//! Deliberately content-agnostic: it scrolls *a sprite*, not "text". The
//! big-text banner is one such sprite, but so is any pre-baked strip, which is
//! why scrolling is separated from glyph rasterisation.

use crate::geometry::Point;
use crate::present::PixelLayer;
use crate::sprite::Sprite;
use crate::surface::Surface;

/// A sprite scrolled leftward across the screen, repeating with wrap-around.
#[derive(Debug, Clone)]
pub struct Marquee {
    sprite: Sprite,
    offset: u32,
    speed: u32,
}

impl Marquee {
    /// Scroll `sprite` by `speed` virtual pixels per [`advance`](Self::advance).
    #[must_use]
    pub fn new(sprite: Sprite, speed: u32) -> Self {
        Self {
            sprite,
            offset: 0,
            speed,
        }
    }

    /// Step the scroll by one frame. Integer steps keep the later upscale a pure
    /// nearest-neighbour (no interpolation blur).
    pub fn advance(&mut self) {
        let period = self.sprite.size().w.max(1);
        self.offset = (self.offset + self.speed) % period;
    }

    #[must_use]
    pub fn offset(&self) -> u32 {
        self.offset
    }
}

impl PixelLayer for Marquee {
    fn render(&self, screen: &mut Surface) {
        let sprite = self.sprite.size();
        let screen_size = screen.size();
        let period = sprite.w.max(1);
        let band_top = centre(screen_size.h, sprite.h);

        for y in 0..sprite.h {
            for x in 0..screen_size.w {
                let src_x = ((x + self.offset) % period) as i32;
                let color = self.sprite.get(Point::new(src_x, y as i32));
                screen.set(Point::new(x as i32, band_top + y as i32), color);
            }
        }
    }
}

/// Offset that centres a strip of height `inner` in `outer` (may be negative if
/// the strip is taller than the screen; the blit clips).
fn centre(outer: u32, inner: u32) -> i32 {
    (outer as i32 - inner as i32) / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::geometry::Size;

    fn strip() -> Sprite {
        // 2×1: opaque red then transparent.
        let mut s = Sprite::new(Size::new(2, 1));
        s.set(Point::new(0, 0), Color::rgb(255, 0, 0));
        s
    }

    #[test]
    fn advance_wraps_modulo_width() {
        let mut m = Marquee::new(strip(), 3);
        assert_eq!(m.offset(), 0);
        m.advance(); // (0 + 3) % 2
        assert_eq!(m.offset(), 1);
        m.advance(); // (1 + 3) % 2
        assert_eq!(m.offset(), 0);
    }

    #[test]
    fn render_tiles_the_strip_and_skips_transparent() {
        let m = Marquee::new(strip(), 1);
        let mut screen = Surface::new(Size::new(4, 1), Color::rgb(0, 0, 0));
        m.render(&mut screen);

        let word = |x: usize| screen.as_slice()[x];
        // offset 0: src cols 0,1,0,1 -> red, (transparent->bg), red, bg.
        assert_eq!(word(0), Color::rgb(255, 0, 0).packed());
        assert_eq!(word(1), Color::rgb(0, 0, 0).packed());
        assert_eq!(word(2), Color::rgb(255, 0, 0).packed());
        assert_eq!(word(3), Color::rgb(0, 0, 0).packed());
    }
}

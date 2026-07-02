//! [`Placard`] — a static sprite parked in the centre of the virtual screen.
//!
//! The still counterpart to [`Marquee`](crate::marquee::Marquee): where the
//! marquee scrolls a strip, a placard centres one sprite on both axes and lets
//! the surface clip anything larger than the screen. Signs like the reject "X"
//! and the "GAME OVER" banner are placards. Whether one is *shown* this frame
//! (e.g. to blink) is the caller's concern — the layer itself is stateless, so
//! the same sprite can be flashed by simply including or omitting it.

use crate::geometry::Point;
use crate::present::PixelLayer;
use crate::sprite::Sprite;
use crate::surface::Surface;

/// A single sprite drawn centred in the screen each frame.
#[derive(Debug, Clone)]
pub struct Placard {
    sprite: Sprite,
}

impl Placard {
    #[must_use]
    pub fn new(sprite: Sprite) -> Self {
        Self { sprite }
    }

    #[must_use]
    pub fn sprite(&self) -> &Sprite {
        &self.sprite
    }
}

impl PixelLayer for Placard {
    fn render(&self, screen: &mut Surface) {
        let s = self.sprite.size();
        let screen_size = screen.size();
        let at = Point::new(centre(screen_size.w, s.w), centre(screen_size.h, s.h));
        screen.draw_sprite(&self.sprite, at);
    }
}

/// Top-left offset that centres `inner` within `outer` (negative when `inner` is
/// larger, so an oversized sprite clips symmetrically rather than shifting).
fn centre(outer: u32, inner: u32) -> i32 {
    (outer as i32 - inner as i32) / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::geometry::Size;

    const INK: Color = Color::rgb(10, 20, 30);

    fn block(w: u32, h: u32) -> Sprite {
        let mut s = Sprite::new(Size::new(w, h));
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                s.set(Point::new(x, y), INK);
            }
        }
        s
    }

    #[test]
    fn centres_a_smaller_sprite() {
        let p = Placard::new(block(2, 2));
        let mut screen = Surface::new(Size::new(4, 4), Color::rgb(0, 0, 0));
        p.render(&mut screen);

        let word = |x: usize, y: usize| screen.as_slice()[y * 4 + x];
        // centre(4,2) = 1: the 2x2 block lands at (1,1)..(2,2).
        assert_eq!(word(1, 1), INK.packed());
        assert_eq!(word(2, 2), INK.packed());
        assert_eq!(word(0, 0), Color::rgb(0, 0, 0).packed()); // margin untouched
    }

    #[test]
    fn oversized_sprite_clips_without_panic() {
        let p = Placard::new(block(6, 6));
        let mut screen = Surface::new(Size::new(4, 4), Color::rgb(0, 0, 0));
        p.render(&mut screen); // centre offset is negative; must not panic
        // The sprite covers the whole screen after clipping.
        assert!(screen.as_slice().iter().all(|&w| w == INK.packed()));
    }
}

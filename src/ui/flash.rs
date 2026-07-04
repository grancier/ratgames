//! [`Flash`] — a translucent full-viewport colour wash (an [`OverlayLayer`]).
//!
//! The arcade "hit flash": a screen-wide tint composited over the game viewport
//! in device space — a green wash on success, red on error, a white blink, a
//! fade-to-black transition. It is a *device-space* overlay (not a pixel layer)
//! because it washes the whole letterboxed game area after the upscale, and its
//! strength is a full alpha byte, not the 1-bit sprite transparency.
//!
//! The wash carries its own alpha as its strength, so animating a flash is just
//! varying that alpha over frames; at zero alpha it draws nothing. `Flash` owns
//! no timing — the caller drives the fade (via [`set_color`](Flash::set_color))
//! from its own frame clock, keeping the widget reusable across any pacing.
//!
//! It tints only the viewport rect handed to [`render`](OverlayLayer::render),
//! never the surrounding letterbox bars, so the wash tracks the game area exactly
//! as the other overlays do.

use crate::color::Color;
use crate::geometry::Rect;
use crate::present::OverlayLayer;
use crate::surface::Surface;

/// A translucent colour wash over the game viewport. The colour's alpha channel
/// is the tint strength; vary it over frames to fade the flash in or out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flash {
    color: Color,
}

impl Flash {
    /// A wash in `color`, whose alpha channel sets the tint strength. A zero-alpha
    /// colour (e.g. [`Color::TRANSPARENT`]) renders nothing — the resting state a
    /// fade returns to.
    #[must_use]
    pub fn new(color: Color) -> Self {
        Self { color }
    }

    /// The current wash colour (its alpha is the tint strength).
    #[must_use]
    pub fn color(&self) -> Color {
        self.color
    }

    /// Set the wash colour — the seam an animation drives each frame to fade the
    /// flash in or out (typically by lowering the alpha toward zero).
    pub fn set_color(&mut self, color: Color) {
        self.color = color;
    }
}

impl OverlayLayer for Flash {
    fn render(&self, window: &mut Surface, viewport: Rect) {
        window.blend_rect(viewport, self.color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Size};

    fn word_at(s: &Surface, x: u32, y: u32) -> u32 {
        s.as_slice()[y as usize * s.size().w as usize + x as usize]
    }

    #[test]
    fn tints_only_the_viewport_leaving_the_letterbox_untouched() {
        let bg = Color::rgb(0, 0, 0);
        let mut window = Surface::new(Size::new(4, 4), bg);
        // The viewport is the inner 2x2 block at (1,1); the border is letterbox.
        let viewport = Rect::new(Point::new(1, 1), Size::new(2, 2));
        Flash::new(Color::argb(128, 255, 255, 255)).render(&mut window, viewport);
        assert_eq!(word_at(&window, 1, 1), 0x0080_8080); // ~50% white over black
        assert_eq!(word_at(&window, 2, 2), 0x0080_8080);
        assert_eq!(word_at(&window, 0, 0), bg.packed()); // letterbox untouched
        assert_eq!(word_at(&window, 3, 3), bg.packed());
    }

    #[test]
    fn a_zero_alpha_flash_draws_nothing() {
        let bg = Color::rgb(10, 20, 30);
        let mut window = Surface::new(Size::new(2, 2), bg);
        Flash::new(Color::argb(0, 255, 0, 0)).render(&mut window, Rect::from_size(Size::new(2, 2)));
        assert_eq!(word_at(&window, 0, 0), bg.packed());
    }

    #[test]
    fn set_color_updates_the_wash() {
        let mut flash = Flash::new(Color::TRANSPARENT);
        flash.set_color(Color::argb(255, 1, 2, 3));
        assert_eq!(flash.color(), Color::argb(255, 1, 2, 3));

        let mut window = Surface::new(Size::new(1, 1), Color::rgb(0, 0, 0));
        flash.render(&mut window, Rect::from_size(Size::new(1, 1)));
        assert_eq!(word_at(&window, 0, 0), Color::rgb(1, 2, 3).packed()); // full alpha
    }
}

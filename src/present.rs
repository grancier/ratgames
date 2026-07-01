//! Composition: the two layer traits and the [`Presentation`] that drives a
//! frame.
//!
//! The whole architecture pivots on keeping two coordinate spaces distinct:
//!
//! * [`PixelLayer`] draws into the low-resolution **virtual screen**. This is
//!   the 8-bit world — rooms, sprites, the marquee. Everything here is later
//!   upscaled by an integer factor, so it stays crisp.
//! * [`OverlayLayer`] draws into the **window** in device pixels, *after* the
//!   upscale. This is for content that must not be pixel-scaled — e.g. the
//!   anti-aliased 20px input font.
//!
//! New capabilities plug in by implementing one of these traits; the compositor
//! never changes (open for extension, closed for modification).

use crate::color::Color;
use crate::geometry::{Point, Rect, Size};
use crate::surface::Surface;

/// Draws into the virtual screen (pixel-art space).
pub trait PixelLayer {
    fn render(&self, screen: &mut Surface);
}

/// Draws into the window (device space), composited after the upscale.
pub trait OverlayLayer {
    /// `viewport` is the letterboxed rect the virtual screen occupies in the
    /// window, so overlays can anchor to the game area rather than raw pixels.
    fn render(&self, window: &mut Surface, viewport: Rect);
}

/// Owns the scratch virtual screen and composites a frame:
/// clear → pixel layers → integer upscale + letterbox → overlay layers.
#[derive(Debug)]
pub struct Presentation {
    screen: Surface,
    backdrop: Color,
    letterbox: Color,
}

impl Presentation {
    #[must_use]
    pub fn new(virtual_size: Size, backdrop: Color, letterbox: Color) -> Self {
        Self {
            screen: Surface::new(virtual_size, backdrop),
            backdrop,
            letterbox,
        }
    }

    #[must_use]
    pub fn virtual_size(&self) -> Size {
        self.screen.size()
    }

    /// Largest integer scale at which the virtual screen fits `window`
    /// (at least 1). Integer-only is the crisp-pixels contract.
    #[must_use]
    pub fn fit_scale(&self, window: Size) -> u32 {
        let v = self.screen.size();
        (window.w / v.w).min(window.h / v.h).max(1)
    }

    /// The centred, letterboxed rect the upscaled screen occupies in `window`.
    #[must_use]
    pub fn viewport(&self, window: Size) -> Rect {
        let v = self.screen.size();
        let scale = self.fit_scale(window);
        let size = Size::new(v.w * scale, v.h * scale);
        let origin = Point::new(
            (window.w.saturating_sub(size.w) / 2) as i32,
            (window.h.saturating_sub(size.h) / 2) as i32,
        );
        Rect::new(origin, size)
    }

    /// Render one frame into `window`.
    pub fn render(
        &mut self,
        world: &[&dyn PixelLayer],
        overlays: &[&dyn OverlayLayer],
        window: &mut Surface,
    ) {
        self.screen.fill(self.backdrop);
        for layer in world {
            layer.render(&mut self.screen);
        }

        let viewport = self.viewport(window.size());
        let scale = self.fit_scale(window.size());
        window.fill(self.letterbox);
        window.draw_upscaled(&self.screen, scale, viewport.origin);

        for overlay in overlays {
            overlay.render(window, viewport);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn presentation(v: Size) -> Presentation {
        Presentation::new(v, Color::rgb(0, 0, 0), Color::rgb(0, 0, 0))
    }

    #[test]
    fn fit_scale_is_integer_and_never_zero() {
        let p = presentation(Size::new(256, 256));
        assert_eq!(p.fit_scale(Size::new(1920, 1080)), 4); // height-bound
        assert_eq!(p.fit_scale(Size::new(768, 768)), 3);
        assert_eq!(p.fit_scale(Size::new(1, 1)), 1);
    }

    #[test]
    fn viewport_centres_the_letterbox() {
        let p = presentation(Size::new(256, 256));
        let vp = p.viewport(Size::new(1920, 1080));
        assert_eq!(vp.size, Size::new(1024, 1024)); // 4x
        assert_eq!(vp.origin, Point::new(448, 28)); // (1920-1024)/2, (1080-1024)/2
    }

    #[test]
    fn render_upscales_pixel_layers_then_calls_overlays() {
        // A 2x2 virtual screen into a 4x4 window: one red pixel at (0,0) should
        // upscale to the top-left 2x2 block; an overlay marks a device pixel.
        struct Dot;
        impl PixelLayer for Dot {
            fn render(&self, screen: &mut Surface) {
                screen.set(Point::new(0, 0), Color::rgb(255, 0, 0));
            }
        }
        struct Mark;
        impl OverlayLayer for Mark {
            fn render(&self, window: &mut Surface, _viewport: Rect) {
                window.set(Point::new(3, 3), Color::rgb(0, 255, 0));
            }
        }

        let mut p = Presentation::new(Size::new(2, 2), Color::rgb(0, 0, 0), Color::rgb(0, 0, 0));
        let mut window = Surface::new(Size::new(4, 4), Color::rgb(0, 0, 0));
        p.render(&[&Dot], &[&Mark], &mut window);

        let word = |x: usize, y: usize| window.as_slice()[y * 4 + x];
        assert_eq!(word(0, 0), Color::rgb(255, 0, 0).packed());
        assert_eq!(word(1, 1), Color::rgb(255, 0, 0).packed());
        assert_eq!(word(2, 0), Color::rgb(0, 0, 0).packed());
        assert_eq!(word(3, 3), Color::rgb(0, 255, 0).packed()); // overlay ran last
    }
}

//! [`ShadowBanner`] — pixel-art text composited in device space with a real drop
//! shadow.
//!
//! The letters are crisp, integer-scaled 8-bit art, exactly as when they were a
//! [`PixelLayer`](ratgames::PixelLayer). The *only* device-space quantity is the
//! drop-shadow offset: a fixed number of real framebuffer pixels. That is why
//! this is an [`OverlayLayer`] and not a pixel layer — an offset of a few device
//! pixels is a fraction of one virtual pixel at the integer upscale, so it cannot
//! be expressed on the virtual grid. The shadow is a yellow silhouette of the
//! glyphs, drawn first; the fill/outline letters are drawn on top, so within the
//! single overlay the z-order (shadow behind, letters in front) is correct.
//!
//! Layout stays anchored to the game's virtual viewport: an [`Anchor::Virtual`]
//! position is given in virtual-screen pixels and projected into device space by
//! the live fit factor, so the HUD and score rows track the window and letterbox
//! exactly as the old pixel composition did. The banner scale likewise multiplies
//! the fit, so a banner is the same size it was as a pixel layer.

use ratgames::{BigText, Color, OverlayLayer, Point, Rect, Size, Sprite, Surface, TextColors};

use crate::config::TextStyle;

/// Where a banner sits within the letterboxed game viewport.
enum Anchor {
    /// Centred on both axes (title, equation, result banner).
    Center,
    /// A fixed top-left position in **virtual-screen** pixels, projected into
    /// device space by the fit factor at render time (HUD, score rows).
    Virtual(Point),
}

/// A pixel-art text banner with a device-space drop shadow. Bakes its glyphs once
/// at source resolution; each frame it recovers the integer fit from the viewport,
/// scales, and blits the shadow silhouette then the letters.
pub struct ShadowBanner {
    /// Source-resolution glyphs (fill + outline), *without* an extruded shadow.
    letters: Sprite,
    /// Device scale = `scale_mult * fit` (banner or HUD magnification).
    scale_mult: u32,
    /// The virtual screen size, to recover the fit factor from the viewport.
    virtual_size: Size,
    anchor: Anchor,
    /// The drop-shadow colour (the palette yellow, matching `BigText`'s own).
    shadow: Color,
    /// Down-right shadow offset, in device pixels.
    shadow_offset_px: i32,
}

impl ShadowBanner {
    /// Bake the glyphs at source resolution with no extruded shadow: the drop
    /// shadow here is a real device-space offset copy, not `BigText`'s block
    /// shadow. The default outline is kept.
    fn bake(text: &str) -> Sprite {
        BigText::new(1).shadow_depth(0).build(text)
    }

    /// A banner centred in the viewport, magnified by the config banner scale.
    #[must_use]
    pub fn centered(text: &str, style: TextStyle, virtual_size: Size) -> Self {
        Self::with_anchor(
            text,
            style.banner_scale,
            Anchor::Center,
            style,
            virtual_size,
        )
    }

    /// A line anchored at a virtual-screen point, magnified by `scale_mult` (the
    /// banner or HUD scale, chosen by the caller per line).
    #[must_use]
    pub fn at_virtual(
        text: &str,
        at: Point,
        scale_mult: u32,
        style: TextStyle,
        virtual_size: Size,
    ) -> Self {
        Self::with_anchor(text, scale_mult, Anchor::Virtual(at), style, virtual_size)
    }

    fn with_anchor(
        text: &str,
        scale_mult: u32,
        anchor: Anchor,
        style: TextStyle,
        virtual_size: Size,
    ) -> Self {
        Self {
            letters: Self::bake(text),
            scale_mult: scale_mult.max(1),
            virtual_size,
            anchor,
            shadow: TextColors::default().shadow,
            shadow_offset_px: style.shadow_offset_px as i32,
        }
    }

    /// The device-space top-left of the letters and the integer scale, for a given
    /// letterboxed `viewport`. The compositor sized the viewport as
    /// `virtual_size * fit`, so dividing recovers exactly the integer scale the
    /// pixel layers were upscaled by.
    fn place(&self, viewport: Rect) -> (Point, u32) {
        let fit = (viewport.size.h / self.virtual_size.h.max(1)).max(1);
        let scale = self.scale_mult * fit;
        let dev = Size::new(self.letters.size().w * scale, self.letters.size().h * scale);
        let origin = match self.anchor {
            Anchor::Center => Point::new(
                viewport.origin.x + centre(viewport.size.w, dev.w),
                viewport.origin.y + centre(viewport.size.h, dev.h),
            ),
            Anchor::Virtual(p) => Point::new(
                viewport.origin.x + p.x * fit as i32,
                viewport.origin.y + p.y * fit as i32,
            ),
        };
        (origin, scale)
    }
}

impl OverlayLayer for ShadowBanner {
    fn render(&self, window: &mut Surface, viewport: Rect) {
        let (origin, scale) = self.place(viewport);
        let shadow_at = Point::new(
            origin.x + self.shadow_offset_px,
            origin.y + self.shadow_offset_px,
        );
        window.draw_sprite_silhouette(&self.letters, scale, shadow_at, self.shadow);
        window.draw_sprite_scaled(&self.letters, scale, origin);
    }
}

/// Top-left offset that centres `inner` within `outer` (negative when `inner` is
/// larger, so an oversized banner clips symmetrically rather than shifting). Mirrors
/// the private helper in `ratgames::placard`, kept local rather than widening the
/// library API for one caller.
fn centre(outer: u32, inner: u32) -> i32 {
    (outer as i32 - inner as i32) / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratgames::palette;

    fn style(offset: u32) -> TextStyle {
        TextStyle {
            banner_scale: 2,
            hud_scale: 1,
            shadow_offset_px: offset,
        }
    }

    /// The bottom-right extent of the pixels satisfying `want`, or `None` if there
    /// are none. Tracks the max x and max y independently.
    fn extent(window: &Surface, want: impl Fn(u32) -> bool) -> Option<(i32, i32)> {
        let s = window.size();
        let (mut mx, mut my) = (-1, -1);
        for y in 0..s.h as i32 {
            for x in 0..s.w as i32 {
                if want(window.as_slice()[y as usize * s.w as usize + x as usize]) {
                    mx = mx.max(x);
                    my = my.max(y);
                }
            }
        }
        (mx >= 0).then_some((mx, my))
    }

    #[test]
    fn place_centres_at_banner_scale_times_fit() {
        // Viewport = virtual * 4, so fit = 4 and a banner_scale-2 banner scales 8x.
        let virtual_size = Size::new(20, 10);
        let vp = Rect::new(Point::new(5, 3), Size::new(80, 40));
        let b = ShadowBanner::centered("A", style(5), virtual_size);
        let (origin, scale) = b.place(vp);
        assert_eq!(scale, 2 * 4);
        let dev = Size::new(b.letters.size().w * scale, b.letters.size().h * scale);
        assert_eq!(
            origin,
            Point::new(5 + centre(80, dev.w), 3 + centre(40, dev.h))
        );
    }

    #[test]
    fn place_projects_a_virtual_anchor_by_the_fit_factor() {
        // A virtual (4,4) anchor at fit 4 lands 16 device px inside the viewport
        // origin, and a HUD-scale-1 line scales by exactly the fit.
        let vp = Rect::new(Point::new(5, 3), Size::new(80, 40));
        let b = ShadowBanner::at_virtual("A", Point::new(4, 4), 1, style(5), Size::new(20, 10));
        let (origin, scale) = b.place(vp);
        assert_eq!(scale, 4);
        assert_eq!(origin, Point::new(5 + 4 * 4, 3 + 4 * 4));
    }

    #[test]
    fn shadow_is_offset_from_the_letters_by_the_device_offset() {
        // Render at fit 1 (viewport == virtual) so the offset is in device px
        // directly. The shadow silhouette is the same glyph shape as the letters,
        // shifted down-right by the offset; its far edges are uncovered by the
        // letters on top, so the shadow's bottom-right extent leads the letters'
        // by exactly the offset — a glyph-independent invariant.
        const OFFSET: i32 = 5;
        let bg = Color::rgb(1, 2, 3);
        let mut window = Surface::new(Size::new(48, 24), bg);
        let vp = Rect::new(Point::ORIGIN, Size::new(48, 24));
        let banner = ShadowBanner::at_virtual(
            "A",
            Point::ORIGIN,
            1,
            style(OFFSET as u32),
            Size::new(48, 24),
        );
        banner.render(&mut window, vp);

        let letters = extent(&window, |w| {
            w == palette::FILL.packed() || w == palette::OUTLINE.packed()
        })
        .expect("letters drawn");
        let shadow = extent(&window, |w| w == palette::SHADOW.packed()).expect("shadow drawn");
        assert_eq!(shadow.0 - letters.0, OFFSET, "shadow leads letters in x");
        assert_eq!(shadow.1 - letters.1, OFFSET, "shadow leads letters in y");
    }

    #[test]
    fn zero_offset_fully_occludes_the_shadow() {
        // With no offset the letters sit exactly over their own silhouette, so no
        // shadow colour survives — the reason the offset must be a real distance.
        let bg = Color::rgb(1, 2, 3);
        let mut window = Surface::new(Size::new(48, 24), bg);
        let vp = Rect::new(Point::ORIGIN, Size::new(48, 24));
        let banner = ShadowBanner::at_virtual("A", Point::ORIGIN, 1, style(0), Size::new(48, 24));
        banner.render(&mut window, vp);
        assert!(
            extent(&window, |w| w == palette::SHADOW.packed()).is_none(),
            "a zero offset leaves no visible shadow"
        );
    }
}

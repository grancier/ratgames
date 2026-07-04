//! [`ShadowBanner`] — pixel-art text composited in device space with a real drop
//! shadow.
//!
//! The letters are crisp, integer-scaled 8-bit art, exactly as when they were a
//! [`PixelLayer`](ratgames::PixelLayer). The *only* device-space quantity is the
//! drop-shadow offset: a fixed number of real framebuffer pixels. That is why
//! this is an [`OverlayLayer`] and not a pixel layer — an offset of a few device
//! pixels is a fraction of one virtual pixel at the integer upscale, so it cannot
//! be expressed on the virtual grid. The shadow is a yellow copy of the glyphs'
//! fill shape, drawn first; the fill + outline letters are drawn on top, so within
//! the single overlay the z-order (shadow behind, letters in front) is correct.
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
/// scales, and blits the shadow copy then the letters.
pub struct ShadowBanner {
    /// Source-resolution glyphs — fill + black outline, no extruded shadow. Drawn
    /// on top.
    letters: Sprite,
    /// The drop shadow: the *fill shape only*, in the shadow colour, on the same
    /// grid as `letters`. Fill-only because a single-colour silhouette of fill +
    /// outline merges the outline into the glyph's counters and reads as a blob;
    /// same grid so it aligns with the letters and the offset stays exact.
    shadow: Sprite,
    /// Device scale = `scale_mult * fit` (banner or HUD magnification).
    scale_mult: u32,
    /// The virtual screen size, to recover the fit factor from the viewport.
    virtual_size: Size,
    anchor: Anchor,
    /// Down-right shadow offset, in device pixels.
    shadow_offset_px: i32,
}

impl ShadowBanner {
    /// The letters: source-resolution glyphs, fill + black outline, no extruded
    /// shadow (the shadow is a real device-space offset copy, not a block shadow).
    fn bake_letters(text: &str) -> Sprite {
        BigText::new(1).shadow_depth(0).build(text)
    }

    /// The shadow: the same glyphs baked fill-only in the shadow colour, on the
    /// same grid as [`bake_letters`](Self::bake_letters) — the outline is recoloured
    /// transparent, which leaves the layout untouched. Copying only the fill keeps
    /// the shadow letter-shaped; recolouring fill + outline into one tone fills the
    /// glyph's counters and reads as a blob.
    fn bake_shadow(text: &str) -> Sprite {
        BigText::new(1)
            .shadow_depth(0)
            .colors(TextColors {
                fill: TextColors::default().shadow,
                outline: Color::TRANSPARENT,
                shadow: Color::TRANSPARENT,
            })
            .build(text)
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
            letters: Self::bake_letters(text),
            shadow: Self::bake_shadow(text),
            scale_mult: scale_mult.max(1),
            virtual_size,
            anchor,
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
        window.draw_sprite_scaled(&self.shadow, scale, shadow_at);
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

    /// Count the sprite pixels satisfying `want`.
    fn count(sprite: &Sprite, want: impl Fn(Color) -> bool) -> usize {
        let s = sprite.size();
        let mut n = 0;
        for y in 0..s.h as i32 {
            for x in 0..s.w as i32 {
                if want(sprite.get(Point::new(x, y))) {
                    n += 1;
                }
            }
        }
        n
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
    fn shadow_is_offset_from_the_fill_by_the_device_offset() {
        // Render at fit 1 (viewport == virtual) so the offset is in device px
        // directly. The shadow is the fill shape in the shadow colour, on the same
        // grid as the letters, shifted down-right by the offset; its bottom-right
        // fill pixels are uncovered by the letters on top, so the shadow's extent
        // leads the green fill's by exactly the offset — a glyph-independent
        // invariant, and proof the shadow tracks the fill (not the outlined blob).
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

        let fill = extent(&window, |w| w == palette::FILL.packed()).expect("letters fill drawn");
        let shadow = extent(&window, |w| w == palette::SHADOW.packed()).expect("shadow drawn");
        assert_eq!(shadow.0 - fill.0, OFFSET, "shadow leads the fill in x");
        assert_eq!(shadow.1 - fill.1, OFFSET, "shadow leads the fill in y");
    }

    #[test]
    fn shadow_copies_the_fill_shape_not_the_outlined_blob() {
        // The shadow sprite has exactly the letters' fill pixels — no outline. A
        // single-colour silhouette of fill + outline would merge the outline into
        // the glyph's counters (e.g. the holes in "8") and read as a blob, so this
        // pins the shadow to the fill shape. Counts are compared on the shared grid.
        let b = ShadowBanner::centered("8", style(5), Size::new(64, 32));
        let fill = count(&b.letters, |c| c == palette::FILL);
        let outline = count(&b.letters, |c| c == palette::OUTLINE);
        let shadow = count(&b.shadow, |c| c.is_visible());
        assert!(
            fill > 0 && outline > 0,
            "letters have both fill and outline"
        );
        assert_eq!(shadow, fill, "shadow copies only the fill, not the outline");
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

//! [`ShadowBanner`] — pixel-art text with a real device-space drop shadow.
//!
//! The banner is crisp, integer-scaled 8-bit art, exactly like a
//! [`PixelLayer`](crate::PixelLayer) banner; the *only* device-space quantity is
//! the drop-shadow offset — a fixed number of real framebuffer pixels. That is why
//! this is an [`OverlayLayer`]: an offset of a few device pixels is a fraction of
//! one virtual pixel at the integer upscale, so it cannot be expressed on the
//! virtual grid. The shadow is a copy of the glyphs' *fill shape* in the shadow
//! colour, drawn first; the fill + outline letters are drawn on top, so within the
//! single overlay the z-order (shadow behind, letters in front) is correct.
//!
//! The shadow copies only the fill, never the outline. A single-colour silhouette
//! of fill + outline merges the outline into the glyph's counters (the holes in
//! `8`, `0`, `A`) and reads as a blob; [`bake_drop_shadow`] avoids that by
//! recolouring the outline transparent while leaving the layout untouched, so the
//! shadow shares the letters' grid and aligns with them pixel-for-pixel.
//!
//! Layout anchors to the game viewport ([`BannerAnchor`]): a [`Virtual`] position
//! is given in virtual-screen pixels and projected into device space by the live
//! fit factor recovered from the viewport, and the scale multiplies that fit — so
//! the banner tracks the window and letterbox exactly as a pixel layer would.
//!
//! [`Virtual`]: BannerAnchor::Virtual

use crate::color::Color;
use crate::geometry::{Point, Rect, Size};
use crate::glyph::GlyphSource;
use crate::present::OverlayLayer;
use crate::sprite::Sprite;
use crate::surface::Surface;
use crate::text::{BigText, TextColors};

/// Bake a pixel-art banner and a matching drop-shadow copy from `text`.
///
/// Returns `(letters, shadow)`: `letters` is `big` baked through `source` (fill +
/// outline), and `shadow` is the same glyphs baked *fill-only* in `shadow` — the
/// fill recoloured to `shadow`, the outline recoloured transparent. Both drop
/// `big`'s extruded block shadow (the real drop shadow replaces it), so the two
/// share an identical grid: the shadow's pixels sit exactly under the letters'
/// fill, keeping the drop offset exact and the shadow letter-shaped.
#[must_use]
pub fn bake_drop_shadow(
    big: &BigText,
    source: &dyn GlyphSource,
    shadow: Color,
    text: &str,
) -> (Sprite, Sprite) {
    let big = big.shadow_depth(0);
    let letters = big.build_with(source, text);
    let shadow = big
        .colors(TextColors {
            fill: shadow,
            outline: Color::TRANSPARENT,
            shadow: Color::TRANSPARENT,
        })
        .build_with(source, text);
    (letters, shadow)
}

/// Where a banner anchors within the letterboxed game viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerAnchor {
    /// Centred on both axes (a title, a result banner, an equation).
    Center,
    /// A fixed top-left position in **virtual-screen** pixels, projected into
    /// device space by the fit factor at render time (a HUD, a score row).
    Virtual(Point),
}

/// A pixel-art banner composited in device space with a real drop shadow, anchored
/// to the game viewport. Holds pre-baked `letters` and `shadow` sprites (see
/// [`bake_drop_shadow`]); each frame it recovers the integer fit from the viewport,
/// scales, and blits the shadow copy then the letters.
#[derive(Debug, Clone)]
pub struct ShadowBanner {
    letters: Sprite,
    shadow: Sprite,
    scale_mult: u32,
    virtual_size: Size,
    anchor: BannerAnchor,
    shadow_offset_px: i32,
}

impl ShadowBanner {
    /// Compose pre-baked `letters` + `shadow` copies (from [`bake_drop_shadow`],
    /// which gives them a shared grid), anchored within a viewport sized against
    /// `virtual_size`. Defaults to a `1×` scale and no offset; set them with
    /// [`scale`](Self::scale) / [`offset`](Self::offset).
    #[must_use]
    pub fn new(letters: Sprite, shadow: Sprite, anchor: BannerAnchor, virtual_size: Size) -> Self {
        Self {
            letters,
            shadow,
            scale_mult: 1,
            virtual_size,
            anchor,
            shadow_offset_px: 0,
        }
    }

    /// Set the device-scale multiplier: the banner is drawn at `mult × fit`, so it
    /// magnifies with the window like a pixel layer. Clamped to at least 1.
    #[must_use]
    pub fn scale(mut self, mult: u32) -> Self {
        self.scale_mult = mult.max(1);
        self
    }

    /// Set the down-right drop-shadow offset, in **device** pixels.
    #[must_use]
    pub fn offset(mut self, device_px: u32) -> Self {
        self.shadow_offset_px = device_px as i32;
        self
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
            BannerAnchor::Center => Point::new(
                viewport.origin.x + centre(viewport.size.w, dev.w),
                viewport.origin.y + centre(viewport.size.h, dev.h),
            ),
            BannerAnchor::Virtual(p) => Point::new(
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
/// larger, so an oversized banner clips symmetrically rather than shifting).
fn centre(outer: u32, inner: u32) -> i32 {
    (outer as i32 - inner as i32) / 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::palette;
    use crate::glyph::Bitmap8x8;

    fn banner(text: &str, anchor: BannerAnchor, virtual_size: Size) -> ShadowBanner {
        let (letters, shadow) =
            bake_drop_shadow(&BigText::new(1), &Bitmap8x8, palette::SHADOW, text);
        ShadowBanner::new(letters, shadow, anchor, virtual_size)
    }

    /// The bottom-right extent of the window pixels equal to `want`, or `None`.
    fn extent(window: &Surface, want: Color) -> Option<(i32, i32)> {
        let s = window.size();
        let (mut mx, mut my) = (-1, -1);
        for y in 0..s.h as i32 {
            for x in 0..s.w as i32 {
                if window.as_slice()[y as usize * s.w as usize + x as usize] == want.packed() {
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
    fn bake_drop_shadow_copies_the_fill_not_the_outlined_blob() {
        // "8" has counters; a fill + outline silhouette would fill them and blob.
        let (letters, shadow) =
            bake_drop_shadow(&BigText::new(1), &Bitmap8x8, palette::SHADOW, "8");
        assert_eq!(letters.size(), shadow.size(), "same grid, so they align");
        let fill = count(&letters, |c| c == palette::FILL);
        let outline = count(&letters, |c| c == palette::OUTLINE);
        let shadow_px = count(&shadow, |c| c.is_visible());
        assert!(
            fill > 0 && outline > 0,
            "letters have both fill and outline"
        );
        assert_eq!(
            shadow_px, fill,
            "shadow copies only the fill, not the outline"
        );
    }

    #[test]
    fn place_centres_at_scale_times_fit() {
        // Viewport = virtual * 4, so fit = 4 and a scale-2 banner scales 8x.
        let vp = Rect::new(Point::new(5, 3), Size::new(80, 40));
        let b = banner("A", BannerAnchor::Center, Size::new(20, 10)).scale(2);
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
        // A virtual (4,4) anchor at fit 4 lands 16 device px inside the origin.
        let vp = Rect::new(Point::new(5, 3), Size::new(80, 40));
        let b = banner(
            "A",
            BannerAnchor::Virtual(Point::new(4, 4)),
            Size::new(20, 10),
        );
        let (origin, scale) = b.place(vp);
        assert_eq!(scale, 4);
        assert_eq!(origin, Point::new(5 + 4 * 4, 3 + 4 * 4));
    }

    #[test]
    fn shadow_is_offset_from_the_fill_by_the_device_offset() {
        // Render at fit 1 (viewport == virtual) so the offset is in device px
        // directly. The shadow is the fill shape, on the letters' grid, shifted
        // down-right; its uncovered bottom-right fill leads the green fill by
        // exactly the offset — glyph-independent, and proof it tracks the fill.
        const OFFSET: u32 = 5;
        let bg = Color::rgb(1, 2, 3);
        let mut window = Surface::new(Size::new(48, 24), bg);
        let vp = Rect::new(Point::ORIGIN, Size::new(48, 24));
        let b = banner("A", BannerAnchor::Virtual(Point::ORIGIN), Size::new(48, 24)).offset(OFFSET);
        b.render(&mut window, vp);

        let fill = extent(&window, palette::FILL).expect("letters fill drawn");
        let shadow = extent(&window, palette::SHADOW).expect("shadow drawn");
        assert_eq!(
            shadow.0 - fill.0,
            OFFSET as i32,
            "shadow leads the fill in x"
        );
        assert_eq!(
            shadow.1 - fill.1,
            OFFSET as i32,
            "shadow leads the fill in y"
        );
    }

    #[test]
    fn zero_offset_fully_occludes_the_shadow() {
        // With no offset the letters sit over their own fill copy, so no shadow
        // colour survives — the reason the offset must be a real device distance.
        let bg = Color::rgb(1, 2, 3);
        let mut window = Surface::new(Size::new(48, 24), bg);
        let vp = Rect::new(Point::ORIGIN, Size::new(48, 24));
        let b = banner("A", BannerAnchor::Virtual(Point::ORIGIN), Size::new(48, 24));
        b.render(&mut window, vp);
        assert!(
            extent(&window, palette::SHADOW).is_none(),
            "a zero offset leaves no visible shadow"
        );
    }
}

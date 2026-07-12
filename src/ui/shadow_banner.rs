//! [`ShadowBanner`] — pixel-art text with a real device-space drop shadow.
//!
//! The banner is crisp, integer-scaled 8-bit art, exactly like a
//! [`PixelLayer`](crate::PixelLayer) banner; the *only* device-space quantity is
//! the drop-shadow offset. That is why this is an [`OverlayLayer`]: the offset is
//! a fraction of one virtual pixel at the integer upscale, so it cannot be
//! expressed on the virtual grid. The shadow is a copy of the glyphs' *fill shape*
//! in the shadow colour, drawn first; the fill + outline letters are drawn on top,
//! so within the single overlay the z-order (shadow behind, letters in front) is
//! correct.
//!
//! The shadow copies only the fill, never the outline. A single-colour silhouette
//! of fill + outline merges the outline into the glyph's counters (the holes in
//! `8`, `0`, `A`) and reads as a blob; [`bake_drop_shadow`] avoids that by
//! recolouring the outline transparent while leaving the layout untouched, so the
//! shadow shares the letters' grid and aligns with them pixel-for-pixel.
//!
//! The offset follows the CSS `text-shadow` model ([`ShadowStyle`]): per-axis
//! [`ShadowLength`]s that are either fixed device pixels or **`em`-relative**. One
//! `em` is the rendered glyph cell height — the source `GlyphSource::cell_height()`
//! baked at the `BigText` scale, then drawn at the device scale — so a single style
//! stays visually proportional across a small HUD row (`scale × 1`) and a large
//! title (`scale × 2`), exactly as relative CSS lengths do. (Blur is intentionally
//! absent: a hard shadow suits the crisp 8-bit look; a soft blur is a separate
//! future effect.)
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

/// A shadow offset length, in the spirit of a CSS `text-shadow` length: either a
/// fixed device-pixel distance or an `em`-relative one that scales with the
/// rendered glyph size. Signed, so a negative offset throws the shadow up/left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShadowLength {
    /// A fixed distance in device (framebuffer) pixels, independent of glyph size.
    DevicePx(i32),
    /// A multiple of one `em` — the rendered glyph cell height — so it scales with
    /// the text. `Em(0.14)` is the `text-shadow: 0.14em` of typography.
    Em(f32),
}

impl ShadowLength {
    /// Resolve to device pixels, given `em_px` (the length of one `em` in device
    /// pixels for the banner being drawn).
    fn resolve(self, em_px: u32) -> i32 {
        match self {
            ShadowLength::DevicePx(px) => px,
            ShadowLength::Em(factor) => (factor * em_px as f32).round() as i32,
        }
    }
}

/// A drop shadow's offset and colour — the CSS `text-shadow` model minus blur
/// (`text-shadow: <offset_x> <offset_y> <color>`, hard-edged).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowStyle {
    pub offset_x: ShadowLength,
    pub offset_y: ShadowLength,
    pub color: Color,
}

impl Default for ShadowStyle {
    /// A modest down-right `0.1em` shadow in the palette shadow colour.
    fn default() -> Self {
        Self {
            offset_x: ShadowLength::Em(0.1),
            offset_y: ShadowLength::Em(0.1),
            color: TextColors::default().shadow,
        }
    }
}

/// A serde config for a [`ShadowStyle`] with `em`-relative offsets — the common
/// case for pixel-art banners, where a proportional shadow scales with the glyph
/// size. A game carries the product values (offsets, colour) in its config and
/// builds a [`ShadowStyle`] with [`style`](Self::style) — the reusable *type* lives
/// here, the *values* live in the game's config. (Fixed device-pixel offsets are
/// still available by constructing a [`ShadowStyle`] directly.)
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ShadowConfig {
    /// Horizontal offset, in em (a fraction of the rendered glyph cell height).
    pub offset_x_em: f32,
    /// Vertical offset, in em.
    pub offset_y_em: f32,
    /// Shadow colour (`#RRGGBB` / `#AARRGGBB`).
    pub color: Color,
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self {
            offset_x_em: 0.14,
            offset_y_em: 0.14,
            // Theme-derived fallback; a game's config carries the product colour.
            color: TextColors::default().shadow,
        }
    }
}

impl ShadowConfig {
    /// The [`ShadowStyle`] these `em`-relative offsets describe.
    #[must_use]
    pub fn style(&self) -> ShadowStyle {
        ShadowStyle {
            offset_x: ShadowLength::Em(self.offset_x_em),
            offset_y: ShadowLength::Em(self.offset_y_em),
            color: self.color,
        }
    }
}

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

/// A pixel-art banner composited in device space with a real, `em`-relative drop
/// shadow, anchored to the game viewport. Bakes `text` once (see
/// [`bake_drop_shadow`]) and remembers the glyph cell height so the shadow offset
/// can resolve against the rendered `em` each frame.
#[derive(Debug, Clone)]
pub struct ShadowBanner {
    letters: Sprite,
    shadow: Sprite,
    /// One `em` in *sprite* pixels: the source glyph cell height times the
    /// `BigText` bake scale. Multiplied by the device scale at render for the
    /// device-space `em`, so the offset tracks the glyph's rendered size whether
    /// magnification comes from `BigText::scale` or the banner's own scale.
    em_base_px: u32,
    scale_mult: u32,
    virtual_size: Size,
    anchor: BannerAnchor,
    offset_x: ShadowLength,
    offset_y: ShadowLength,
}

impl ShadowBanner {
    /// Bake `text` (via `big` / `source`) into a banner with a `style` drop shadow,
    /// anchored within a viewport sized against `virtual_size`. Defaults to a `1×`
    /// scale; set it with [`scale`](Self::scale).
    #[must_use]
    pub fn new(
        text: &str,
        big: &BigText,
        source: &dyn GlyphSource,
        style: ShadowStyle,
        anchor: BannerAnchor,
        virtual_size: Size,
    ) -> Self {
        let (letters, shadow) = bake_drop_shadow(big, source, style.color, text);
        Self {
            letters,
            shadow,
            em_base_px: source.cell_height() * big.scale(),
            scale_mult: 1,
            virtual_size,
            anchor,
            offset_x: style.offset_x,
            offset_y: style.offset_y,
        }
    }

    /// Set the device-scale multiplier: the banner is drawn at `mult × fit`, so it
    /// magnifies with the window like a pixel layer. Clamped to at least 1.
    #[must_use]
    pub fn scale(mut self, mult: u32) -> Self {
        self.scale_mult = mult.max(1);
        self
    }

    /// The device-space top-left of the letters and the integer scale, for a given
    /// letterboxed `viewport`.
    fn place(&self, viewport: Rect) -> (Point, u32) {
        place_in_viewport(
            viewport,
            self.virtual_size,
            self.letters.size(),
            self.scale_mult,
            self.anchor,
        )
    }
}

/// Bakes pixel-art [`ShadowBanner`]s that share a glyph source, drop shadow, and
/// virtual screen — the constants of one screen's banners — so a caller sets them
/// once and then builds each banner from just its text, place, and magnification.
///
/// This is the reusable composition a game's banner helpers wrap: the toolkit owns
/// *how* a banner is baked (glyphs at `BigText` source-scale 1, then device-scaled,
/// with an `em`-relative shadow), while the game keeps its own scale and anchor
/// choices — product values — and calls the factory to build. Holds the source by
/// reference, so it is a short-lived builder (make one while assembling a screen).
pub struct ShadowBannerFactory<'a> {
    source: &'a dyn GlyphSource,
    shadow: ShadowStyle,
    virtual_size: Size,
}

impl<'a> ShadowBannerFactory<'a> {
    /// A factory baking through `source` with `shadow`, anchored within a viewport
    /// sized against `virtual_size`.
    #[must_use]
    pub fn new(source: &'a dyn GlyphSource, shadow: ShadowStyle, virtual_size: Size) -> Self {
        Self {
            source,
            shadow,
            virtual_size,
        }
    }

    /// A centred banner magnified by `scale`.
    #[must_use]
    pub fn centered(&self, text: &str, scale: u32) -> ShadowBanner {
        self.bake(text, BannerAnchor::Center, scale)
    }

    /// A banner anchored at a virtual-screen point, magnified by `scale`.
    #[must_use]
    pub fn at(&self, text: &str, at: Point, scale: u32) -> ShadowBanner {
        self.bake(text, BannerAnchor::Virtual(at), scale)
    }

    fn bake(&self, text: &str, anchor: BannerAnchor, scale: u32) -> ShadowBanner {
        ShadowBanner::new(
            text,
            &BigText::new(1),
            self.source,
            self.shadow,
            anchor,
            self.virtual_size,
        )
        .scale(scale)
    }
}

/// The device-space top-left and integer scale to place `content` (a baked
/// sprite's size) within a letterboxed `viewport`, given the virtual screen size,
/// a `scale_mult`, and an `anchor`. Shared by the banner and blink overlays so
/// device-space content tracks the window/letterbox identically. The compositor
/// sized the viewport as `virtual_size * fit`, so dividing recovers exactly the
/// integer scale the pixel layers were upscaled by.
pub(crate) fn place_in_viewport(
    viewport: Rect,
    virtual_size: Size,
    content: Size,
    scale_mult: u32,
    anchor: BannerAnchor,
) -> (Point, u32) {
    let fit = (viewport.size.h / virtual_size.h.max(1)).max(1);
    let scale = scale_mult * fit;
    let dev = Size::new(content.w * scale, content.h * scale);
    let origin = match anchor {
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

impl OverlayLayer for ShadowBanner {
    fn render(&self, window: &mut Surface, viewport: Rect) {
        let (origin, scale) = self.place(viewport);
        // One em is the rendered glyph cell height (source cell × BigText scale ×
        // device scale), so the shadow offset scales with the banner: a bigger
        // title gets a proportionally longer shadow.
        let em_px = self.em_base_px * scale;
        let shadow_at = Point::new(
            origin.x + self.offset_x.resolve(em_px),
            origin.y + self.offset_y.resolve(em_px),
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

/// The game's pixel-art text style: how far its display banners and body lines
/// are magnified, and how their drop shadow is styled. The reusable *type*
/// lives here beside the factory that consumes it; a game's chosen scales live
/// in its config, and the neutral default is identity magnification.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BannerStyle {
    /// Source-pixel magnification for display banners (titles, verdicts, the
    /// challenge prompt).
    pub banner_scale: u32,
    /// Smaller magnification for body lines (a score / lives HUD, list rows).
    pub hud_scale: u32,
    /// The banners' drop-shadow style.
    pub shadow: ShadowConfig,
}

impl Default for BannerStyle {
    fn default() -> Self {
        // Neutral: identity magnification. A game's chosen scales live in its
        // config.
        Self {
            banner_scale: 1,
            hud_scale: 1,
            shadow: ShadowConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::palette;
    use crate::glyph::Bitmap8x8;

    fn style(offset: ShadowLength) -> ShadowStyle {
        ShadowStyle {
            offset_x: offset,
            offset_y: offset,
            color: palette::SHADOW,
        }
    }

    fn banner(
        text: &str,
        anchor: BannerAnchor,
        virtual_size: Size,
        style: ShadowStyle,
    ) -> ShadowBanner {
        ShadowBanner::new(
            text,
            &BigText::new(1),
            &Bitmap8x8,
            style,
            anchor,
            virtual_size,
        )
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
    fn shadow_length_resolves_em_and_device_px() {
        assert_eq!(ShadowLength::Em(0.5).resolve(8), 4);
        assert_eq!(ShadowLength::Em(0.14).resolve(64), 9); // round(8.96)
        assert_eq!(ShadowLength::DevicePx(5).resolve(999), 5); // ignores em
        assert_eq!(ShadowLength::DevicePx(-3).resolve(10), -3); // negative allowed
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
        let b = banner(
            "A",
            BannerAnchor::Center,
            Size::new(20, 10),
            ShadowStyle::default(),
        )
        .scale(2);
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
            ShadowStyle::default(),
        );
        let (origin, scale) = b.place(vp);
        assert_eq!(scale, 4);
        assert_eq!(origin, Point::new(5 + 4 * 4, 3 + 4 * 4));
    }

    /// The device offset the shadow leads the letters' fill by, for a banner baked
    /// with `big` at `scale_mult`, rendered at `fit 1` (viewport == virtual) so the
    /// offset is read directly.
    fn shadow_lead(big: &BigText, offset: ShadowLength, scale_mult: u32) -> (i32, i32) {
        let vs = Size::new(128, 64);
        let bg = Color::rgb(1, 2, 3);
        let mut window = Surface::new(vs, bg);
        let vp = Rect::new(Point::ORIGIN, vs);
        ShadowBanner::new(
            "A",
            big,
            &Bitmap8x8,
            style(offset),
            BannerAnchor::Virtual(Point::ORIGIN),
            vs,
        )
        .scale(scale_mult)
        .render(&mut window, vp);
        let fill = extent(&window, palette::FILL).expect("letters fill drawn");
        let shadow = extent(&window, palette::SHADOW).expect("shadow drawn");
        (shadow.0 - fill.0, shadow.1 - fill.1)
    }

    #[test]
    fn em_shadow_scales_with_the_banner_scale() {
        // One em = cell (8) * BigText scale (1) * device scale. At fit 1, device
        // scale = scale_mult, so a 0.5em shadow is 4 px at scale 1 and 8 px at
        // scale 2 — same proportion, size-appropriate. The point of the em unit.
        let big = BigText::new(1);
        assert_eq!(shadow_lead(&big, ShadowLength::Em(0.5), 1), (4, 4));
        assert_eq!(shadow_lead(&big, ShadowLength::Em(0.5), 2), (8, 8));
    }

    #[test]
    fn em_accounts_for_the_bigtext_bake_scale() {
        // The em base must fold in BigText's own scale, not just the source cell:
        // BigText::new(2) bakes a glyph cell twice as tall, so the same 0.5em is
        // twice the device offset as BigText::new(1) at the same banner scale.
        // (The bug this guards: em resolved off the unscaled 8px cell → 4px, when
        // the rendered cell is 16px → 8px.)
        assert_eq!(
            shadow_lead(&BigText::new(1), ShadowLength::Em(0.5), 1),
            (4, 4)
        );
        assert_eq!(
            shadow_lead(&BigText::new(2), ShadowLength::Em(0.5), 1),
            (8, 8)
        );
    }

    #[test]
    fn device_px_shadow_is_size_independent() {
        // A fixed device offset stays put regardless of scale — the escape hatch
        // for when a constant framebuffer distance is genuinely wanted.
        let big = BigText::new(1);
        assert_eq!(shadow_lead(&big, ShadowLength::DevicePx(5), 1), (5, 5));
        assert_eq!(shadow_lead(&big, ShadowLength::DevicePx(5), 2), (5, 5));
    }

    #[test]
    fn zero_offset_fully_occludes_the_shadow() {
        // With no offset the letters sit over their own fill copy, so no shadow
        // colour survives — the reason the offset must be a real device distance.
        let vs = Size::new(48, 24);
        let bg = Color::rgb(1, 2, 3);
        let mut window = Surface::new(vs, bg);
        let vp = Rect::new(Point::ORIGIN, vs);
        banner(
            "A",
            BannerAnchor::Virtual(Point::ORIGIN),
            vs,
            style(ShadowLength::Em(0.0)),
        )
        .render(&mut window, vp);
        assert!(
            extent(&window, palette::SHADOW).is_none(),
            "a zero offset leaves no visible shadow"
        );
    }

    #[test]
    fn shadow_config_builds_an_em_style_and_round_trips() {
        let config = ShadowConfig {
            offset_x_em: 0.05,
            offset_y_em: 0.08,
            color: Color::rgb(0xF2, 0xC9, 0x4C),
        };
        let built = config.style();
        assert_eq!(built.offset_x, ShadowLength::Em(0.05));
        assert_eq!(built.offset_y, ShadowLength::Em(0.08));
        assert_eq!(built.color, Color::rgb(0xF2, 0xC9, 0x4C));

        let text = serde_json::to_string(&config).expect("serialize");
        let parsed: ShadowConfig = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, config);
        // A sparse config fills every field from the default.
        let defaulted: ShadowConfig = serde_json::from_str("{}").expect("deserialize empty");
        assert_eq!(defaulted, ShadowConfig::default());
    }

    #[test]
    fn factory_bakes_the_same_banners_as_direct_construction() {
        let vs = Size::new(64, 32);
        let vp = Rect::new(Point::ORIGIN, vs);
        let shadow = ShadowStyle {
            offset_x: ShadowLength::Em(0.1),
            offset_y: ShadowLength::Em(0.1),
            color: palette::SHADOW,
        };
        let factory = ShadowBannerFactory::new(&Bitmap8x8, shadow, vs);

        // A rendered surface is the observable: the factory's `centered` / `at`
        // must match a hand-built `ShadowBanner` with the same anchor and scale.
        let render = |banner: &ShadowBanner| {
            let mut surface = Surface::new(vs, Color::rgb(0, 0, 0));
            banner.render(&mut surface, vp);
            surface.as_slice().to_vec()
        };

        let direct_centered = ShadowBanner::new(
            "A",
            &BigText::new(1),
            &Bitmap8x8,
            shadow,
            BannerAnchor::Center,
            vs,
        )
        .scale(2);
        assert_eq!(render(&factory.centered("A", 2)), render(&direct_centered));

        let at = Point::new(3, 5);
        let direct_at = ShadowBanner::new(
            "A",
            &BigText::new(1),
            &Bitmap8x8,
            shadow,
            BannerAnchor::Virtual(at),
            vs,
        )
        .scale(2);
        assert_eq!(render(&factory.at("A", at, 2)), render(&direct_at));
    }

    #[test]
    fn banner_style_round_trips_with_an_identity_default() {
        let style = BannerStyle::default();
        assert_eq!((style.banner_scale, style.hud_scale), (1, 1));
        let text = serde_json::to_string(&style).expect("serialize");
        let parsed: BannerStyle = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, style);
        let defaulted: BannerStyle = serde_json::from_str("{}").expect("deserialize empty");
        assert_eq!(defaulted, BannerStyle::default());
    }
}

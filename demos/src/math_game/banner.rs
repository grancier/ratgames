//! A static big-text banner, baked to a `Sprite` on demand — the example-local
//! counterpart of the retired `ratgames::BannerConfig`.
//!
//! A specific banner's text, scale, and colours are *product* choices, so they
//! live in the example rather than the library. `ratgames` supplies the generic
//! [`BigText`] baker and the [`GlyphSource`] the letters draw through; this only
//! bundles the knobs and calls it.

use ratgames::{BigText, GlyphSource, Sprite, TextColors};

/// A styled block of oversized pixel-art text. Fields mirror the [`BigText`]
/// builder; [`sprite`](Self::sprite) bakes them through a glyph source.
pub struct Banner {
    pub text: String,
    /// Source-pixel magnification. Keep it small when the glyph source is already
    /// high-resolution — `scale` magnifies source pixels, it is *not* resolution.
    pub scale: u32,
    pub tracking: u32,
    pub shadow_depth: u32,
    pub outline_px: u32,
    pub gap: u32,
    pub colors: TextColors,
}

impl Banner {
    /// Bake the banner into a sprite through `source`.
    ///
    /// Unlike `BannerConfig::sprite`, this skips the footprint size guard: an
    /// example bakes trusted, hardcoded text, so the ceiling the library enforces
    /// at its untrusted-config boundary is unnecessary here.
    #[must_use]
    pub fn sprite(&self, source: &dyn GlyphSource) -> Sprite {
        BigText::new(self.scale)
            .tracking(self.tracking)
            .shadow_depth(self.shadow_depth)
            .outline(self.outline_px)
            .gap(self.gap)
            .colors(self.colors)
            .build_with(source, &self.text)
    }
}

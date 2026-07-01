//! Packed 32-bit colour.
//!
//! Stored as `0xAARRGGBB`. Pixel-art sprites use 1-bit alpha (a cell is either
//! [`Color::TRANSPARENT`] or fully opaque); the overlay text pipeline uses the
//! full alpha byte for anti-aliased coverage. Presenting to the framebuffer
//! drops alpha via [`Color::packed`] because minifb expects `0x00RRGGBB`.

/// An `0xAARRGGBB` colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color(u32);

impl Color {
    /// Fully transparent — sprite cells with this value are not drawn.
    pub const TRANSPARENT: Color = Color(0x0000_0000);

    /// An opaque colour from 8-bit channels.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color(0xFF00_0000 | ((r as u32) << 16) | ((g as u32) << 8) | b as u32)
    }

    /// A colour with an explicit alpha (used by the AA text overlay).
    #[must_use]
    pub const fn argb(a: u8, r: u8, g: u8, b: u8) -> Self {
        Color(((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32)
    }

    #[must_use]
    pub const fn alpha(self) -> u8 {
        (self.0 >> 24) as u8
    }

    /// Whether the pixel should be drawn at all (any non-zero alpha).
    #[must_use]
    pub const fn is_visible(self) -> bool {
        self.alpha() != 0
    }

    /// The `0x00RRGGBB` word the framebuffer expects (alpha discarded).
    #[must_use]
    pub const fn packed(self) -> u32 {
        self.0 & 0x00FF_FFFF
    }
}

/// The single source of truth for the retro palette. Named by role, not by hue,
/// so a re-theme touches one place.
pub mod palette {
    use super::Color;

    /// Backdrop of the virtual screen behind everything.
    pub const BG: Color = Color::rgb(0x18, 0x18, 0x30);
    /// Big-text letter body.
    pub const FILL: Color = Color::rgb(0x39, 0xD3, 0x53);
    /// Big-text outline / border.
    pub const OUTLINE: Color = Color::rgb(0x00, 0x00, 0x00);
    /// Big-text extruded 3D shadow.
    pub const SHADOW: Color = Color::rgb(0xF2, 0xC4, 0x0C);
    /// Bars around the letterboxed screen in the window.
    pub const LETTERBOX: Color = Color::rgb(0x00, 0x00, 0x00);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_is_opaque_and_packs_channels() {
        let c = Color::rgb(0x39, 0xD3, 0x53);
        assert_eq!(c.alpha(), 0xFF);
        assert_eq!(c.packed(), 0x0039_D353);
        assert!(c.is_visible());
    }

    #[test]
    fn transparent_is_invisible_and_packs_to_zero() {
        assert!(!Color::TRANSPARENT.is_visible());
        assert_eq!(Color::TRANSPARENT.packed(), 0);
    }

    #[test]
    fn packed_discards_alpha() {
        assert_eq!(Color::argb(0x80, 0x11, 0x22, 0x33).packed(), 0x0011_2233);
    }
}

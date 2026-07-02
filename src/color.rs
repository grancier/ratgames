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

impl serde::Serialize for Color {
    /// As `#RRGGBB` when opaque, else `#AARRGGBB`.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let hex = if self.alpha() == 0xFF {
            format!("#{:06X}", self.packed())
        } else {
            format!("#{:08X}", self.0)
        };
        serializer.serialize_str(&hex)
    }
}

impl<'de> serde::Deserialize<'de> for Color {
    /// From `#RRGGBB` (opaque) or `#AARRGGBB`; the leading `#` is optional.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let s = String::deserialize(deserializer)?;
        let hex = s.strip_prefix('#').unwrap_or(&s);
        let value = u32::from_str_radix(hex, 16)
            .map_err(|e| D::Error::custom(format!("invalid colour hex {s:?}: {e}")))?;
        match hex.len() {
            6 => Ok(Color(0xFF00_0000 | value)),
            8 => Ok(Color(value)),
            _ => Err(D::Error::custom(format!(
                "colour must be #RRGGBB or #AARRGGBB, got {s:?}"
            ))),
        }
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

    /// UI accent — the input panel's nested border (light blue).
    pub const ACCENT: Color = Color::rgb(0x87, 0xCE, 0xFA);
    /// Error / rejection — the reject cross (red).
    pub const DANGER: Color = Color::rgb(0xE0, 0x2C, 0x2C);
    /// Alert — the "GAME OVER" sign (yellow).
    pub const WARNING: Color = Color::rgb(0xFF, 0xE8, 0x5C);
    /// Foreground text on UI panels — the input line (near-white).
    pub const INK: Color = Color::rgb(0xF0, 0xF0, 0xF0);
    /// UI panel background — behind the input line (near-black).
    pub const PANEL: Color = Color::rgb(0x0A, 0x0A, 0x14);
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

    #[test]
    fn serde_hex_round_trips_and_rejects_junk() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct W {
            c: Color,
        }

        // Opaque colour serialises as #RRGGBB and round-trips.
        let w = W {
            c: Color::rgb(0x39, 0xD3, 0x53),
        };
        let text = toml::to_string(&w).expect("serialize");
        assert!(text.contains("\"#39D353\""), "got {text}");
        assert_eq!(toml::from_str::<W>(&text).expect("parse"), w);

        // Explicit alpha survives via #AARRGGBB.
        let a: W = toml::from_str("c = \"#80112233\"").expect("parse argb");
        assert_eq!(a.c, Color::argb(0x80, 0x11, 0x22, 0x33));

        // Bad hex is rejected, not silently defaulted.
        assert!(toml::from_str::<W>("c = \"#12\"").is_err());
        assert!(toml::from_str::<W>("c = \"nope\"").is_err());
    }
}

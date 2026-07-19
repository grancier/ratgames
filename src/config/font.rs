//! Font and glyph-source configuration.
//!
//! The `FontConfig`/`FontSource` selectors for the input overlay, and the
//! `GlyphSourceConfig` a banner's letters rasterise through.

use std::path::PathBuf;

use crate::font::{FontError, SystemFont};
use crate::glyph::{Bitmap8x8, GlyphSource, RasterGlyphSource};

/// Font selection for the input overlay.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FontConfig {
    /// On-screen size, in device pixels — never scaled with the pixel world.
    pub size_px: f32,
    pub source: FontSource,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            size_px: 20.0,
            source: FontSource::default(),
        }
    }
}

/// Where the input font comes from. In TOML: `kind = "system" | "file" |
/// "embedded"` plus the variant's field.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FontSource {
    /// A family resolved from the OS font database, optionally narrowed to a
    /// specific installed face by `weight`/`style`/`stretch`.
    ///
    /// `family` is a [`FontFamily`]: [`FontFamily::Default`] (the value when
    /// `family` is omitted) is the platform's generic monospace — there *is* a
    /// font, just not a named product face — while [`FontFamily::Named`] selects
    /// a specific family. No product font is baked into the library default; a
    /// named family belongs in a caller's config. Either way selection picks a
    /// **real** face: `fontdue` does not synthesise faux bold/italic, so the
    /// family must actually ship the requested face. An unavailable combination
    /// is not an error — `fontdb` falls back to the nearest installed face (e.g.
    /// `weight = "medium"` on a family with no Medium face resolves to Regular).
    /// All three styles refine [`Default`]'s Normal.
    System {
        #[serde(default)]
        family: FontFamily,
        #[serde(default)]
        weight: FontWeight,
        #[serde(default)]
        style: FontStyle,
        #[serde(default)]
        stretch: FontStretch,
    },
    /// A `.ttf`/`.ttc` at an explicit path (the file already pins one face).
    File { path: PathBuf },
    /// The crate-bundled DejaVu Sans Mono. Needs neither a filesystem nor a
    /// system font database, so it is the font source for the WebAssembly /
    /// browser target; also the deterministic face behind non-`#[ignore]`d
    /// glyph tests. Two faces ship — Regular and Bold — and `weight` selects the
    /// nearer of the two (the `Embedded` parallel to `System` letting `fontdb`
    /// pick the nearest installed face). In TOML/JSON: `kind = "embedded"` with
    /// an optional `weight` (`"bold"` for the heavier face; omitted = Regular).
    Embedded {
        #[serde(default)]
        weight: FontWeight,
    },
}

impl Default for FontSource {
    fn default() -> Self {
        FontSource::System {
            family: FontFamily::Default,
            weight: FontWeight::default(),
            style: FontStyle::default(),
            stretch: FontStretch::default(),
        }
    }
}

/// Which OS font family a [`FontSource::System`] resolves.
///
/// `Default` is the platform's generic monospace — there *is* a font, it is
/// simply not a named product face — and is the value when `family` is omitted.
/// `Named` selects a specific installed family. In TOML/JSON write the family
/// name as a string, or the reserved string `"default"` (equivalently, omit
/// `family`) for the generic monospace.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FontFamily {
    /// The platform's generic monospace.
    #[default]
    Default,
    /// A specific installed family, by name.
    Named(String),
}

/// The reserved config string denoting [`FontFamily::Default`].
const DEFAULT_FONT_FAMILY: &str = "default";

impl serde::Serialize for FontFamily {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            FontFamily::Default => DEFAULT_FONT_FAMILY,
            FontFamily::Named(name) => name.as_str(),
        })
    }
}

impl<'de> serde::Deserialize<'de> for FontFamily {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let name = String::deserialize(deserializer)?;
        Ok(if name == DEFAULT_FONT_FAMILY {
            FontFamily::Default
        } else {
            FontFamily::Named(name)
        })
    }
}

/// A font weight on the OS/2 `usWeightClass` scale (1–1000).
///
/// Deserialises from **either** a name (`"bold"`) **or** a raw number (`700`);
/// both select the same installed face. The nine standard steps are `thin`
/// (100), `extra_light` (200), `light` (300), `normal` (400), `medium` (500),
/// `semi_bold` (600), `bold` (700), `extra_bold` (800), `black` (900). A value
/// equal to a standard step serialises back to its name; any other in-range
/// value serialises as the number. Unknown names and out-of-range numbers are
/// rejected at parse time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontWeight(pub u16);

impl Default for FontWeight {
    fn default() -> Self {
        FontWeight(400)
    }
}

impl FontWeight {
    /// The canonical name of a standard weight step, if `self` is one.
    fn name(self) -> Option<&'static str> {
        Some(match self.0 {
            100 => "thin",
            200 => "extra_light",
            300 => "light",
            400 => "normal",
            500 => "medium",
            600 => "semi_bold",
            700 => "bold",
            800 => "extra_bold",
            900 => "black",
            _ => return None,
        })
    }

    /// The numeric weight for a standard step name.
    fn from_name(name: &str) -> Option<FontWeight> {
        let value = match name {
            "thin" => 100,
            "extra_light" => 200,
            "light" => 300,
            "normal" => 400,
            "medium" => 500,
            "semi_bold" => 600,
            "bold" => 700,
            "extra_bold" => 800,
            "black" => 900,
            _ => return None,
        };
        Some(FontWeight(value))
    }

    /// A raw weight number, accepted only within the valid `1..=1000` range.
    fn from_number(value: u64) -> Option<FontWeight> {
        if (1..=1000).contains(&value) {
            Some(FontWeight(value as u16))
        } else {
            None
        }
    }
}

impl serde::Serialize for FontWeight {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.name() {
            Some(name) => serializer.serialize_str(name),
            None => serializer.serialize_u16(self.0),
        }
    }
}

impl<'de> serde::Deserialize<'de> for FontWeight {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct WeightVisitor;

        impl<'de> serde::de::Visitor<'de> for WeightVisitor {
            type Value = FontWeight;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a weight name such as \"bold\", or a number in 1..=1000")
            }

            fn visit_str<E>(self, value: &str) -> Result<FontWeight, E>
            where
                E: serde::de::Error,
            {
                FontWeight::from_name(value)
                    .ok_or_else(|| E::custom(format!("unknown font weight {value:?}")))
            }

            fn visit_u64<E>(self, value: u64) -> Result<FontWeight, E>
            where
                E: serde::de::Error,
            {
                FontWeight::from_number(value)
                    .ok_or_else(|| E::custom(format!("font weight {value} out of range 1..=1000")))
            }

            fn visit_i64<E>(self, value: i64) -> Result<FontWeight, E>
            where
                E: serde::de::Error,
            {
                let n = u64::try_from(value)
                    .map_err(|_| E::custom(format!("font weight {value} out of range 1..=1000")))?;
                self.visit_u64(n)
            }
        }

        deserializer.deserialize_any(WeightVisitor)
    }
}

/// A font slant, matching a real installed face (no synthesised oblique).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

/// A font width (condensed…expanded), matching a real installed face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FontStretch {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    #[default]
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

/// Which glyph source a banner's letters rasterise through: the chunky `font8x8`
/// bitmap (the default) or a higher-resolution TTF.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
pub enum GlyphSourceConfig {
    /// The `font8x8` 8×8 bitmap source.
    #[serde(rename = "bitmap8x8")]
    #[default]
    Bitmap8x8,
    /// A TTF rasterised at `cell_px` source-pixels and thresholded to 1-bit at
    /// `threshold` (coverage ≥ `threshold` becomes ink). The scalars are declared
    /// before `font` so they precede the sub-table in TOML.
    #[serde(rename = "raster")]
    Raster {
        cell_px: u32,
        #[serde(default = "raster_default_threshold")]
        threshold: u8,
        font: FontSource,
    },
}

/// Default coverage threshold for a raster glyph source (coverage ≥ 128 = ink).
/// Matches [`RasterGlyphSource`](crate::glyph::RasterGlyphSource)'s own default,
/// so an omitted `threshold` in TOML preserves the current look.
fn raster_default_threshold() -> u8 {
    128
}

impl GlyphSourceConfig {
    /// Build the glyph source, loading a font for the raster variant.
    ///
    /// # Errors
    /// Returns [`FontError`] if the raster variant's font cannot be loaded.
    pub fn resolve(&self) -> Result<Box<dyn GlyphSource>, FontError> {
        match self {
            Self::Bitmap8x8 => Ok(Box::new(Bitmap8x8)),
            Self::Raster {
                cell_px,
                threshold,
                font,
            } => {
                let loaded = SystemFont::from_source(font)?;
                Ok(Box::new(
                    RasterGlyphSource::new(loaded, *cell_px).with_threshold(*threshold),
                ))
            }
        }
    }
}

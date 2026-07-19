//! Anti-aliased glyph rasterisation for the input overlay.
//!
//! Discovery via `fontdb` (system fonts by family), rasterisation via `fontdue`
//! (char → 8-bit coverage). We do not hand-roll hinting or AA. This is the
//! smooth-text path; it is entirely separate from the pixel-art `font8x8` path.

use crate::config::{FontConfig, FontFamily, FontSource, FontStretch, FontStyle, FontWeight};

/// DejaVu Sans Mono (Regular), bundled for [`FontSource::Embedded`] so the
/// anti-aliased glyph pipeline works with no filesystem and no system font
/// database — the WebAssembly / browser target, where both `std::fs` and
/// `fontdb::Database::load_system_fonts` are unavailable.
///
/// This is deliberately *not* a product default: [`FontSource::default`] stays
/// the platform's generic monospace (memory: no baked-in product face).
/// `Embedded` is an explicit opt-in for fontless environments, and it needs one
/// concrete, license-safe face — DejaVu ships under the Bitstream Vera license,
/// which permits redistribution (see `assets/fonts/DejaVuSansMono.LICENSE`).
const EMBEDDED_FONT: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");

/// A loaded, size-agnostic font ready to rasterise glyphs.
pub struct SystemFont {
    font: fontdue::Font,
}

impl std::fmt::Debug for SystemFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SystemFont").finish_non_exhaustive()
    }
}

impl SystemFont {
    /// Load the font described by `config`.
    ///
    /// # Errors
    /// Fails if the family cannot be resolved, a file cannot be read, or the
    /// font data cannot be parsed.
    pub fn load(config: &FontConfig) -> Result<Self, FontError> {
        Self::from_source(&config.source)
    }

    /// Load a font directly from a [`FontSource`], independent of any pixel size
    /// (size is applied per glyph at rasterise time). Shared by the input
    /// overlay and the raster glyph source.
    ///
    /// # Errors
    /// As [`load`](Self::load).
    pub fn from_source(source: &FontSource) -> Result<Self, FontError> {
        let (bytes, index) = match source {
            FontSource::System {
                family,
                weight,
                style,
                stretch,
            } => {
                let family = match family {
                    FontFamily::Default => None,
                    FontFamily::Named(name) => Some(name.as_str()),
                };
                resolve_system(family, *weight, *style, *stretch)?
            }
            FontSource::File { path } => {
                let bytes = std::fs::read(path).map_err(|e| FontError::Io {
                    path: path.clone(),
                    source: e,
                })?;
                (bytes, 0)
            }
            FontSource::Embedded => (EMBEDDED_FONT.to_vec(), 0),
        };

        let settings = fontdue::FontSettings {
            collection_index: index,
            ..fontdue::FontSettings::default()
        };
        let font = fontdue::Font::from_bytes(bytes, settings)
            .map_err(|e| FontError::Parse(e.to_string()))?;
        Ok(Self { font })
    }

    /// Vertical metrics at `px`, with a sane fallback if the face omits them.
    #[must_use]
    pub fn line_metrics(&self, px: f32) -> LineMetrics {
        self.font.horizontal_line_metrics(px).map_or(
            LineMetrics {
                ascent: px * 0.8,
                descent: -px * 0.2,
                line_height: px * 1.2,
            },
            |m| LineMetrics {
                ascent: m.ascent,
                descent: m.descent,
                line_height: m.new_line_size,
            },
        )
    }

    /// Rasterise `ch` at `px` into an 8-bit coverage bitmap plus placement.
    #[must_use]
    pub fn rasterize(&self, ch: char, px: f32) -> RasterGlyph {
        let (m, coverage) = self.font.rasterize(ch, px);
        RasterGlyph {
            width: m.width,
            height: m.height,
            advance: m.advance_width,
            xmin: m.xmin,
            ymin: m.ymin,
            coverage,
        }
    }
}

/// Resolve a system face at the requested weight/style/stretch to owned font
/// bytes and a face index. A named `family` falls back to any monospace; an
/// unnamed one (`None`) asks for the platform's generic monospace directly — no
/// product font is assumed. `fontdb` returns the nearest installed face when the
/// exact combination is unavailable, so this never fails on style alone — only
/// when no family matches at all.
fn resolve_system(
    family: Option<&str>,
    weight: FontWeight,
    style: FontStyle,
    stretch: FontStretch,
) -> Result<(Vec<u8>, u32), FontError> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    let families = match family {
        Some(name) => vec![fontdb::Family::Name(name), fontdb::Family::Monospace],
        None => vec![fontdb::Family::Monospace],
    };
    let query = fontdb::Query {
        families: &families,
        weight: fontdb::Weight(weight.0),
        stretch: stretch_to_fontdb(stretch),
        style: style_to_fontdb(style),
    };
    let id = db
        .query(&query)
        .ok_or_else(|| FontError::NotFound(family.unwrap_or("monospace").to_string()))?;
    db.with_face_data(id, |data, index| (data.to_vec(), index))
        .ok_or(FontError::NoData)
}

/// Map the config slant onto `fontdb`'s.
fn style_to_fontdb(style: FontStyle) -> fontdb::Style {
    match style {
        FontStyle::Normal => fontdb::Style::Normal,
        FontStyle::Italic => fontdb::Style::Italic,
        FontStyle::Oblique => fontdb::Style::Oblique,
    }
}

/// Map the config width onto `fontdb`'s (`ttf_parser::Width`).
fn stretch_to_fontdb(stretch: FontStretch) -> fontdb::Stretch {
    match stretch {
        FontStretch::UltraCondensed => fontdb::Stretch::UltraCondensed,
        FontStretch::ExtraCondensed => fontdb::Stretch::ExtraCondensed,
        FontStretch::Condensed => fontdb::Stretch::Condensed,
        FontStretch::SemiCondensed => fontdb::Stretch::SemiCondensed,
        FontStretch::Normal => fontdb::Stretch::Normal,
        FontStretch::SemiExpanded => fontdb::Stretch::SemiExpanded,
        FontStretch::Expanded => fontdb::Stretch::Expanded,
        FontStretch::ExtraExpanded => fontdb::Stretch::ExtraExpanded,
        FontStretch::UltraExpanded => fontdb::Stretch::UltraExpanded,
    }
}

/// Vertical layout metrics, y-up (ascent positive, descent negative).
#[derive(Debug, Clone, Copy)]
pub struct LineMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_height: f32,
}

/// A rasterised glyph: coverage bitmap plus pen-relative placement.
#[derive(Debug, Clone)]
pub struct RasterGlyph {
    pub width: usize,
    pub height: usize,
    /// Horizontal pen advance, in device pixels.
    pub advance: f32,
    /// Left side bearing from the pen origin.
    pub xmin: i32,
    /// Bottom of the bitmap relative to the baseline (y-up).
    pub ymin: i32,
    /// Row-major coverage, `0..=255`, length `width * height`.
    pub coverage: Vec<u8>,
}

/// Errors loading a font.
#[derive(Debug, thiserror::Error)]
pub enum FontError {
    #[error("no system font matched family {0:?}")]
    NotFound(String),
    #[error("font face data was unavailable")]
    NoData,
    #[error("failed to read font file {path:?}: {source}")]
    Io {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse font: {0}")]
    Parse(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `Embedded` source loads the bundled DejaVu Sans Mono — no filesystem,
    /// no system font database. This is the wasm/browser path, and being
    /// deterministic (the face ships in the crate) it is *not* `#[ignore]`d.
    #[test]
    fn embedded_source_loads_bundled_font() {
        let cfg = FontConfig {
            source: FontSource::Embedded,
            ..FontConfig::default()
        };
        let font = SystemFont::load(&cfg).expect("the bundled DejaVu Sans Mono loads");
        assert!(
            font.line_metrics(32.0).ascent > 0.0,
            "the bundled face reports a positive ascent"
        );
    }

    /// The embedded face rasterises real ink — the whole AA glyph path exercised
    /// with zero host dependencies, so it stands in for the `#[ignore]`d
    /// system-font coverage on any machine (and in CI / wasm).
    #[test]
    fn embedded_source_rasterizes_ink() {
        let font = SystemFont::from_source(&FontSource::Embedded).expect("bundled face");
        let g = font.rasterize('A', 32.0);
        assert!(g.width > 0 && g.height > 0, "glyph has a bitmap");
        assert!(g.coverage.iter().any(|&c| c > 0), "glyph has coverage");
        // DejaVu Sans Mono is monospace: 'A' and 'i' share one pen advance.
        let i = font.rasterize('i', 32.0);
        assert_eq!(
            g.advance, i.advance,
            "a monospace face advances every glyph equally"
        );
    }

    #[test]
    fn default_source_is_generic_monospace() {
        // No product font is baked into the library default: an unnamed system
        // family resolves to the platform's generic monospace.
        assert!(matches!(
            FontSource::default(),
            FontSource::System {
                family: FontFamily::Default,
                ..
            }
        ));
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn loads_system_monospace_and_rasterizes() {
        let font = SystemFont::load(&FontConfig::default()).expect("a system monospace font");
        let g = font.rasterize('A', 20.0);
        assert!(g.width > 0 && g.height > 0, "glyph has a bitmap");
        assert!(g.coverage.iter().any(|&c| c > 0), "glyph has coverage");
        assert!(font.line_metrics(20.0).ascent > 0.0, "positive ascent");
    }

    /// A styled query resolves a *distinct* installed face: Menlo ships a real
    /// Bold, so the same glyph rasterises to different coverage than Regular.
    /// (Menlo is monospace, so the pen advance is unchanged — only the ink is.)
    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn styled_query_selects_a_distinct_face() {
        let styled = |weight, style| {
            SystemFont::from_source(&FontSource::System {
                family: FontFamily::Named("Menlo".to_string()),
                weight,
                style,
                stretch: FontStretch::Normal,
            })
        };
        let regular = styled(FontWeight(400), FontStyle::Normal).expect("Menlo Regular");
        let bold = styled(FontWeight(700), FontStyle::Normal).expect("Menlo Bold");

        let a_regular = regular.rasterize('A', 32.0);
        let a_bold = bold.rasterize('A', 32.0);
        assert!(a_bold.coverage.iter().any(|&c| c > 0), "bold glyph has ink");
        assert_ne!(
            a_regular.coverage, a_bold.coverage,
            "bold selects a heavier face, not synthesised from Regular"
        );
    }
}

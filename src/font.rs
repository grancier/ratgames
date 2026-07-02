//! Anti-aliased glyph rasterisation for the input overlay.
//!
//! Discovery via `fontdb` (system fonts by family), rasterisation via `fontdue`
//! (char → 8-bit coverage). We do not hand-roll hinting or AA. This is the
//! smooth-text path; it is entirely separate from the pixel-art `font8x8` path.

use crate::config::{FontConfig, FontSource};

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
            FontSource::System { family } => resolve_system(family)?,
            FontSource::File { path } => {
                let bytes = std::fs::read(path).map_err(|e| FontError::Io {
                    path: path.clone(),
                    source: e,
                })?;
                (bytes, 0)
            }
            FontSource::Embedded => return Err(FontError::NoEmbedded),
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

/// Resolve `family` (or any monospace) to owned font bytes and a face index.
fn resolve_system(family: &str) -> Result<(Vec<u8>, u32), FontError> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    let families = [fontdb::Family::Name(family), fontdb::Family::Monospace];
    let query = fontdb::Query {
        families: &families,
        weight: fontdb::Weight::NORMAL,
        stretch: fontdb::Stretch::Normal,
        style: fontdb::Style::Normal,
    };
    let id = db
        .query(&query)
        .ok_or_else(|| FontError::NotFound(family.to_string()))?;
    db.with_face_data(id, |data, index| (data.to_vec(), index))
        .ok_or(FontError::NoData)
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
    #[error("no embedded font is bundled")]
    NoEmbedded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_source_reports_missing_asset() {
        let cfg = FontConfig {
            source: FontSource::Embedded,
            ..FontConfig::default()
        };
        assert!(matches!(SystemFont::load(&cfg), Err(FontError::NoEmbedded)));
    }

    #[test]
    fn default_source_is_a_monospace_system_family() {
        assert!(matches!(
            FontSource::default(),
            FontSource::System { family } if family == "Menlo"
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
}

//! Central configuration — the in-code "header" of defaults.
//!
//! Nothing downstream hardcodes a dimension, colour, or size; it all flows from
//! here. `Default` supplies the built-in values, colours flow from [`Theme`], and
//! the whole tree is `serde`-serialisable so [`Config::load`] can read a TOML or
//! JSON file — falling back to a default for every field the file omits.
//!
//! Field order note: within a struct, scalar fields are declared before any
//! nested-struct field, because TOML requires a table's values to precede its
//! sub-tables when serialised.

use std::path::{Path, PathBuf};

use crate::font::FontError;
use crate::glyph::GlyphSource;
use crate::text::BigText;
use crate::theme::Theme;

mod defaults;
mod device;
mod font;
mod layout;
mod quiz;

pub use device::*;
pub use font::*;
pub use layout::*;
pub use quiz::*;

/// The whole app's tunables in one tree.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Config {
    pub theme: Theme,
    pub window: WindowConfig,
    pub screen: ScreenConfig,
    pub marquee: MarqueeConfig,
    pub input: InputConfig,
    pub quiz: QuizConfig,
}

/// Errors loading a [`Config`] from a file.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config {path:?}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse config {path:?}: {source}")]
    Parse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("failed to parse JSON config {path:?}: {source}")]
    ParseJson {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("invalid config: {0}")]
    Invalid(String),
    #[error("failed to load banner font: {0}")]
    Font(#[from] FontError),
    #[error("banner sprite too large: {kind} area {area} exceeds the {limit} limit")]
    SpriteTooLarge {
        kind: &'static str,
        area: u64,
        limit: u64,
    },
}

/// The serialisation formats [`Config::load`] accepts, selected by file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfigFormat {
    Toml,
    Json,
}

impl ConfigFormat {
    /// Pick a format from `path`'s extension, before the file is read.
    fn from_path(path: &Path) -> Result<Self, ConfigError> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("toml") => Ok(Self::Toml),
            Some("json") => Ok(Self::Json),
            other => Err(ConfigError::Invalid(format!(
                "unsupported config extension {other:?} for {path:?}; use .toml or .json"
            ))),
        }
    }

    /// Deserialise `text` in this format into a [`Config`].
    fn parse(self, text: &str, path: &Path) -> Result<Config, ConfigError> {
        match self {
            Self::Toml => toml::from_str(text).map_err(|source| ConfigError::Parse {
                path: path.to_path_buf(),
                source,
            }),
            Self::Json => serde_json::from_str(text).map_err(|source| ConfigError::ParseJson {
                path: path.to_path_buf(),
                source,
            }),
        }
    }
}

impl Config {
    /// Load and validate a config from `path`, choosing TOML or JSON by the file
    /// extension (`.toml` / `.json`). Every field the file omits falls back to its
    /// default, so a partial file is fine.
    ///
    /// # Errors
    /// [`ConfigError::Invalid`] if the extension is unsupported or a value is out
    /// of range, [`ConfigError::Io`] if the file cannot be read, or
    /// [`ConfigError::Parse`] / [`ConfigError::ParseJson`] if the contents are not
    /// valid for this schema.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let format = ConfigFormat::from_path(path)?;
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let config = format.parse(&text, path)?;
        config.validate()?;
        Ok(config)
    }

    /// Check the cross-cutting invariants the type system does not: non-zero
    /// dimensions, a scale floor of at least 1, and a fractional/positive input
    /// panel. Called by [`load`](Self::load).
    ///
    /// # Errors
    /// Returns [`ConfigError::Invalid`] describing the first violation found.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let win = self.window.size();
        if win.w == 0 || win.h == 0 {
            return Err(ConfigError::Invalid(format!(
                "window size must be non-zero, got {}x{}",
                win.w, win.h
            )));
        }
        // Every virtual screen the window can switch to across a breakpoint must
        // be non-zero, not just the base size.
        for (label, size) in [
            ("screen.size", self.screen.size),
            (
                "screen.mobile_size",
                self.screen.size_for(DeviceClass::Mobile),
            ),
            (
                "screen.tablet_size",
                self.screen.size_for(DeviceClass::Tablet),
            ),
        ] {
            if size.w == 0 || size.h == 0 {
                return Err(ConfigError::Invalid(format!(
                    "{label} must be non-zero, got {}x{}",
                    size.w, size.h
                )));
            }
        }
        if self.screen.min_scale == 0 {
            return Err(ConfigError::Invalid(
                "screen.min_scale must be at least 1".to_string(),
            ));
        }
        let hf = self.input.height_fraction;
        if !hf.is_finite() || hf <= 0.0 || hf > 1.0 {
            return Err(ConfigError::Invalid(format!(
                "input.height_fraction must be in (0, 1], got {hf}"
            )));
        }
        let px = self.input.font.size_px;
        if !px.is_finite() || px <= 0.0 {
            return Err(ConfigError::Invalid(format!(
                "input.font.size_px must be positive, got {px}"
            )));
        }

        if self.marquee.text_scale == 0 {
            return Err(ConfigError::Invalid(
                "marquee.text_scale must be at least 1".to_string(),
            ));
        }
        validate_glyph_source(&self.marquee.glyph_source, "marquee")?;

        for (name, banner) in [
            ("quiz.cross", &self.quiz.cross),
            ("quiz.game_over", &self.quiz.game_over),
        ] {
            if banner.scale == 0 {
                return Err(ConfigError::Invalid(format!(
                    "{name}.scale must be at least 1"
                )));
            }
            validate_glyph_source(&banner.glyph_source, name)?;
        }

        Ok(())
    }
}

/// Safety ceiling on a raster glyph's `cell_px`. Not a styling choice: it bounds
/// the per-glyph rasterisation allocation (~`cell_px²`) so a mistyped size cannot
/// exhaust memory while a banner's footprint is measured.
const MAX_CELL_PX: u32 = 2048;

/// Reject an out-of-range raster `cell_px`; bitmap sources have nothing to check.
pub(crate) fn validate_glyph_source(
    source: &GlyphSourceConfig,
    ctx: &str,
) -> Result<(), ConfigError> {
    if let GlyphSourceConfig::Raster { cell_px, .. } = source {
        if *cell_px == 0 {
            return Err(ConfigError::Invalid(format!(
                "{ctx}.glyph_source.cell_px must be at least 1"
            )));
        }
        if *cell_px > MAX_CELL_PX {
            return Err(ConfigError::Invalid(format!(
                "{ctx}.glyph_source.cell_px {cell_px} exceeds the {MAX_CELL_PX} safety limit"
            )));
        }
    }
    Ok(())
}

/// Safety ceilings on a baked banner, guarding the two `Vec` allocations in
/// [`BigText::build_with`](crate::text::BigText::build_with): the source-grid and
/// the final scaled sprite. Not styling — a banner this large is a mistyped
/// config, so it is rejected before allocating.
const MAX_SOURCE_SPRITE_CELLS: u64 = 4_000_000;
const MAX_SCALED_SPRITE_PIXELS: u64 = 16_000_000;

/// Reject a banner whose bake would exceed the pre-allocation limits, measured
/// via [`BigText::footprint`] before any large `Vec` is created.
pub(crate) fn guard_footprint(
    text: &BigText,
    source: &dyn GlyphSource,
    s: &str,
) -> Result<(), ConfigError> {
    let fp = text.footprint(source, s);
    if fp.source_cells > MAX_SOURCE_SPRITE_CELLS {
        return Err(ConfigError::SpriteTooLarge {
            kind: "source",
            area: fp.source_cells,
            limit: MAX_SOURCE_SPRITE_CELLS,
        });
    }
    if fp.scaled_pixels > MAX_SCALED_SPRITE_PIXELS {
        return Err(ConfigError::SpriteTooLarge {
            kind: "scaled",
            area: fp.scaled_pixels,
            limit: MAX_SCALED_SPRITE_PIXELS,
        });
    }
    Ok(())
}

/// Where a runtime [`Config`] is loaded from. Game config comes only from files
/// (or a Rust-defined default) — never environment variables — so this is either
/// an explicit `--config <path>` file or the built-in default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    /// Load and validate the config file (TOML or JSON) at this path.
    File(PathBuf),
    /// Use the built-in [`Config::default`].
    Default,
}

impl ConfigSource {
    /// Resolve the source: the `--config <path>` file if one was given, else the
    /// built-in default. The caller reads the CLI at the edge and passes it in.
    #[must_use]
    pub fn resolve(cli_path: Option<PathBuf>) -> Self {
        match cli_path {
            Some(path) => Self::File(path),
            None => Self::Default,
        }
    }

    /// Materialise the config: load and validate a file source, or return the
    /// built-in defaults.
    ///
    /// # Errors
    /// Returns [`ConfigError`] if a file source cannot be read, parsed, or fails
    /// [`Config::validate`].
    pub fn load(&self) -> Result<Config, ConfigError> {
        self.load_or_else(Config::default)
    }

    /// Like [`load`](Self::load), but the [`Default`](Self::Default) source falls
    /// back to `default()` — letting an example supply its own baseline (e.g. a
    /// Rust-defined preset) while still honouring an explicit `--config` file.
    ///
    /// # Errors
    /// As [`load`](Self::load): a file source that cannot be read, parsed, or
    /// validated.
    pub fn load_or_else<F>(&self, default: F) -> Result<Config, ConfigError>
    where
        F: FnOnce() -> Config,
    {
        match self {
            Self::File(path) => Config::load(path),
            Self::Default => Ok(default()),
        }
    }
}

/// Extract `--config <path>` (or `--config=<path>`) from `args`, returning the
/// path if present and the remaining positional arguments in order. A minimal
/// hand-rolled parser: the CLI surface is one flag plus optional banner text,
/// which does not warrant a parsing framework.
///
/// # Errors
/// Returns [`ConfigError::Invalid`] if `--config` appears without a path.
pub fn parse_config_flag<I>(args: I) -> Result<(Option<PathBuf>, Vec<String>), ConfigError>
where
    I: IntoIterator<Item = String>,
{
    let mut config: Option<PathBuf> = None;
    let mut positionals: Vec<String> = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if let Some(path) = arg.strip_prefix("--config=") {
            config = Some(PathBuf::from(path));
        } else if arg == "--config" {
            let path = args.next().ok_or_else(|| {
                ConfigError::Invalid("--config requires a path argument".to_string())
            })?;
            config = Some(PathBuf::from(path));
        } else {
            positionals.push(arg);
        }
    }
    Ok((config, positionals))
}

#[cfg(test)]
mod tests;

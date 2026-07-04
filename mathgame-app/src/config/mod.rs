//! The app's configuration: `ratgames::Config` (the engine) plus this app's own
//! pixel-art text style — sourced from data, not hardcoded in Rust.
//!
//! The default lives in a bundled `defaults.json`, embedded at compile time and
//! parsed once, so `cargo run -p mathgame-app` needs no external file yet no
//! product value — the Menlo input font, its size, the banner/HUD scale and
//! shadow depth — is baked into a Rust literal. A `--config <path>` flag
//! overrides it with a TOML or JSON file (chosen by extension), exactly like the
//! ratgames examples. Rust holds only the config *types* and their `Default`
//! fallbacks, never the product choices themselves.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use ratgames::{Color, Config, ConfigError, ShadowLength, ShadowStyle, TextColors};

/// The app's pixel-art text style: how far the banners and HUD are magnified and
/// how their drop shadow is styled. App-specific — there is no home for it in
/// `ratgames::Config` — so it rides alongside the engine config here.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct TextStyle {
    /// Source-pixel magnification for the title / result / equation banners.
    pub banner_scale: u32,
    /// Smaller magnification for the score / lives HUD line.
    pub hud_scale: u32,
    /// The banner drop-shadow style.
    pub shadow: ShadowConfig,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            banner_scale: 2,
            hud_scale: 1,
            shadow: ShadowConfig::default(),
        }
    }
}

/// The banner drop-shadow style, sourced from data. Modelled on CSS `text-shadow`:
/// per-axis offsets and a colour. Offsets are **em-relative** — a fraction of the
/// rendered glyph cell height — so one style stays visually proportional whether
/// it drives the small HUD row or the large title, rather than looking oversized
/// on one and undersized on the other as a fixed pixel offset would.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct ShadowConfig {
    /// Horizontal offset, in em (a fraction of the glyph cell height).
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
            // Theme-derived fallback; the bundled JSON carries the product colour.
            color: TextColors::default().shadow,
        }
    }
}

impl ShadowConfig {
    /// The `ratgames` render style this config maps to: em-relative offsets.
    #[must_use]
    pub fn style(&self) -> ShadowStyle {
        ShadowStyle {
            offset_x: ShadowLength::Em(self.offset_x_em),
            offset_y: ShadowLength::Em(self.offset_y_em),
            color: self.color,
        }
    }
}

/// High-score board settings: how many places it keeps and where it is saved.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct ScoresConfig {
    /// Maximum entries kept on the board (the "top N").
    pub capacity: usize,
    /// File the board is persisted to, relative to the working directory.
    pub file: PathBuf,
}

impl Default for ScoresConfig {
    fn default() -> Self {
        Self {
            capacity: 10,
            file: PathBuf::from("mathgame-highscores.json"),
        }
    }
}

/// Per-answer feedback style. A correct answer washes the screen with
/// `correct_color` (a translucent tint that fades out); a wrong answer flashes a
/// solid reject cross in `wrong_color`, `flashes` times at `cross_scale`, then
/// shows the verdict. `duration_frames` is how long the verdict holds. All frame
/// counts are at the window's `target_fps`. Sourced from data, like the rest of
/// the app's look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct FeedbackConfig {
    /// Screen wash on a correct answer (`#AARRGGBB`; the alpha is the strength).
    pub correct_color: Color,
    /// The reject-cross colour on a wrong answer (drawn solid, so alpha is moot).
    pub wrong_color: Color,
    /// How many frames the verdict holds before advancing.
    pub duration_frames: u32,
    /// Source-pixel magnification of the reject-cross "X" glyph.
    pub cross_scale: u32,
    /// How many times the reject cross blinks.
    pub flashes: u32,
    /// Frames the cross is shown, and hidden, in each blink.
    pub flash_frames: u32,
}

impl Default for FeedbackConfig {
    fn default() -> Self {
        Self {
            // Palette-derived fallbacks; the bundled JSON carries the product
            // colours. (`FILL` green wash at ~60% alpha, solid `DANGER` red X.)
            correct_color: Color::argb(0x99, 0x39, 0xD3, 0x53),
            wrong_color: Color::rgb(0xE0, 0x2C, 0x2C),
            duration_frames: 30,
            cross_scale: 8,
            flashes: 3,
            flash_frames: 6,
        }
    }
}

/// The whole app config: the reusable engine config plus this app's text style,
/// per-answer feedback, and high-score settings.
#[derive(Debug, Clone, PartialEq, Default, serde::Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Window, screen, theme, and the anti-aliased input font.
    pub engine: Config,
    /// Pixel-art banner / HUD style.
    pub text: TextStyle,
    /// Correct / wrong answer feedback colours and timing.
    pub feedback: FeedbackConfig,
    /// High-score board capacity and save file.
    pub scores: ScoresConfig,
}

/// Errors materialising an [`AppConfig`].
#[derive(Debug, thiserror::Error)]
pub enum AppConfigError {
    #[error("failed to read config {path:?}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse TOML config {path:?}: {source}")]
    ParseToml {
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
    #[error(transparent)]
    Engine(#[from] ConfigError),
}

/// The bundled default, parsed once. A malformed bundle is caught by the unit
/// test below (a build-time guarantee), not left as a runtime risk.
static BUNDLED: LazyLock<AppConfig> = LazyLock::new(|| {
    serde_json::from_str(include_str!("defaults.json"))
        .expect("bundled config/defaults.json must be valid")
});

impl AppConfig {
    /// The config for this run: the `--config <path>` file if one was given, else
    /// the bundled default. Both are validated before use.
    ///
    /// # Errors
    /// [`AppConfigError`] if a file source cannot be read, parsed, or fails
    /// validation.
    pub fn resolve(cli_path: Option<PathBuf>) -> Result<Self, AppConfigError> {
        let config = match cli_path {
            Some(path) => Self::load_file(&path)?,
            None => BUNDLED.clone(),
        };
        config.validate()?;
        Ok(config)
    }

    /// Read and parse a config file, choosing TOML or JSON by its extension.
    fn load_file(path: &Path) -> Result<Self, AppConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| AppConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        match path.extension().and_then(|e| e.to_str()) {
            Some("toml") => toml::from_str(&text).map_err(|source| AppConfigError::ParseToml {
                path: path.to_path_buf(),
                source,
            }),
            Some("json") => {
                serde_json::from_str(&text).map_err(|source| AppConfigError::ParseJson {
                    path: path.to_path_buf(),
                    source,
                })
            }
            other => Err(AppConfigError::Invalid(format!(
                "unsupported config extension {other:?} for {path:?}; use .toml or .json"
            ))),
        }
    }

    /// The app's own invariants plus the engine's. `Config::validate` covers the
    /// window / screen / input font; here we add the text-style scales (a `0`
    /// magnification would silently render nothing).
    fn validate(&self) -> Result<(), AppConfigError> {
        if self.text.banner_scale == 0 {
            return Err(AppConfigError::Invalid(
                "text.banner_scale must be at least 1".to_string(),
            ));
        }
        if self.text.hud_scale == 0 {
            return Err(AppConfigError::Invalid(
                "text.hud_scale must be at least 1".to_string(),
            ));
        }
        if self.scores.capacity == 0 {
            return Err(AppConfigError::Invalid(
                "scores.capacity must be at least 1".to_string(),
            ));
        }
        if self.scores.file.as_os_str().is_empty() {
            return Err(AppConfigError::Invalid(
                "scores.file must not be empty".to_string(),
            ));
        }
        if !self.text.shadow.offset_x_em.is_finite() || !self.text.shadow.offset_y_em.is_finite() {
            return Err(AppConfigError::Invalid(
                "text.shadow.offset_x_em / offset_y_em must be finite".to_string(),
            ));
        }
        if self.feedback.duration_frames == 0 {
            return Err(AppConfigError::Invalid(
                "feedback.duration_frames must be at least 1".to_string(),
            ));
        }
        if self.feedback.cross_scale == 0 {
            return Err(AppConfigError::Invalid(
                "feedback.cross_scale must be at least 1".to_string(),
            ));
        }
        if self.feedback.flashes == 0 {
            return Err(AppConfigError::Invalid(
                "feedback.flashes must be at least 1".to_string(),
            ));
        }
        if self.feedback.flash_frames == 0 {
            return Err(AppConfigError::Invalid(
                "feedback.flash_frames must be at least 1".to_string(),
            ));
        }
        self.engine.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratgames::{FontFamily, FontSource};

    #[test]
    fn bundled_default_selects_menlo_and_the_shipped_text_style() {
        // The bundled JSON is the source of truth for the product look, not a Rust
        // literal: the Menlo input font and the banner/HUD style come from data.
        let config = AppConfig::resolve(None).expect("bundled config must be valid");
        assert_eq!(config.engine.input.font.size_px, 44.0);
        match config.engine.input.font.source {
            FontSource::System {
                family: FontFamily::Named(name),
                ..
            } => assert_eq!(name, "Menlo"),
            other => panic!("expected a named system font, got {other:?}"),
        }
        assert_eq!(
            config.text,
            TextStyle {
                banner_scale: 2,
                hud_scale: 1,
                shadow: ShadowConfig {
                    offset_x_em: 0.2,
                    offset_y_em: 0.2,
                    color: Color::rgb(0xF2, 0xC9, 0x4C),
                },
            }
        );
        assert_eq!(
            config.feedback,
            FeedbackConfig {
                correct_color: Color::argb(0x99, 0x39, 0xD3, 0x53),
                wrong_color: Color::rgb(0xE0, 0x2C, 0x2C),
                duration_frames: 30,
                cross_scale: 8,
                flashes: 3,
                flash_frames: 6,
            }
        );
        assert_eq!(config.scores.capacity, 10);
        assert_eq!(
            config.scores.file,
            std::path::PathBuf::from("mathgame-highscores.json")
        );
    }

    #[test]
    fn rust_default_stays_generic_monospace() {
        // The Rust `Default` is only the serde fallback for omitted fields; the
        // named face lives in the bundled data, never in a Rust literal.
        assert_eq!(
            AppConfig::default().engine.input.font.source,
            FontSource::default()
        );
    }

    #[test]
    fn zero_banner_scale_is_rejected() {
        let config = AppConfig {
            text: TextStyle {
                banner_scale: 0,
                ..TextStyle::default()
            },
            ..AppConfig::default()
        };
        assert!(matches!(config.validate(), Err(AppConfigError::Invalid(_))));
    }

    #[test]
    fn zero_scores_capacity_is_rejected() {
        let config = AppConfig {
            scores: ScoresConfig {
                capacity: 0,
                ..ScoresConfig::default()
            },
            ..AppConfig::default()
        };
        assert!(matches!(config.validate(), Err(AppConfigError::Invalid(_))));
    }

    #[test]
    fn zero_feedback_duration_is_rejected() {
        let config = AppConfig {
            feedback: FeedbackConfig {
                duration_frames: 0,
                ..FeedbackConfig::default()
            },
            ..AppConfig::default()
        };
        assert!(matches!(config.validate(), Err(AppConfigError::Invalid(_))));
    }
}

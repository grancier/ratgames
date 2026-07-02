//! Central configuration — the in-code "header" of defaults.
//!
//! Nothing downstream hardcodes a dimension, colour, or size; it all flows from
//! here. `Default` supplies the built-in values, colours flow from [`Theme`], and
//! the whole tree is `serde`-serialisable so [`Config::load`] can read a TOML
//! file — falling back to a default for every field the file omits.
//!
//! Field order note: within a struct, scalar fields are declared before any
//! nested-struct field, because TOML requires a table's values to precede its
//! sub-tables when serialised.

use std::path::{Path, PathBuf};

use crate::color::Color;
use crate::geometry::{Point, Rect, Size};
use crate::sprite::Sprite;
use crate::text::{BigText, TextColors};
use crate::theme::Theme;

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
    #[error("invalid config: {0}")]
    Invalid(String),
}

impl Config {
    /// Load and validate a TOML config from `path`. Every field the file omits
    /// falls back to its default, so a partial file is fine.
    ///
    /// # Errors
    /// Returns [`ConfigError::Io`] if the file cannot be read,
    /// [`ConfigError::Parse`] if it is not valid TOML for this schema, or
    /// [`ConfigError::Invalid`] if a value is out of range (see
    /// [`validate`](Self::validate)).
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let config: Config = toml::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
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
        if self.window.width == 0 || self.window.height == 0 {
            return Err(ConfigError::Invalid(format!(
                "window size must be non-zero, got {}x{}",
                self.window.width, self.window.height
            )));
        }
        if self.screen.size.w == 0 || self.screen.size.h == 0 {
            return Err(ConfigError::Invalid(format!(
                "screen size must be non-zero, got {}x{}",
                self.screen.size.w, self.screen.size.h
            )));
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
        Ok(())
    }
}

/// Physical window.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub target_fps: usize,
    pub resizable: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "ratgames".to_string(),
            width: 768,
            height: 768,
            target_fps: 60,
            resizable: true,
        }
    }
}

/// The low-resolution virtual screen the pixel world composes into.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ScreenConfig {
    pub backdrop: Color,
    pub letterbox: Color,
    /// Crisp-clip floor for the integer upscale: the virtual screen is never
    /// presented below this factor (a smaller window clips instead of blurring).
    pub min_scale: u32,
    /// Declared last: a sub-table must follow this struct's scalar fields in TOML.
    pub size: Size,
}

impl Default for ScreenConfig {
    fn default() -> Self {
        let theme = Theme::default();
        Self {
            backdrop: theme.background,
            letterbox: theme.letterbox,
            min_scale: 1,
            size: Size::new(256, 256),
        }
    }
}

/// The scrolling big-text banner.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MarqueeConfig {
    pub text_scale: u32,
    pub tracking: u32,
    pub shadow_depth: u32,
    /// Outline thickness around the letters, in source pixels (`0` = none).
    pub outline_px: u32,
    pub gap: u32,
    pub speed: u32,
    pub colors: TextColors,
}

impl Default for MarqueeConfig {
    fn default() -> Self {
        Self {
            text_scale: 6,
            tracking: 1,
            shadow_depth: 3,
            outline_px: 1,
            gap: 14,
            speed: 2,
            colors: TextColors::default(),
        }
    }
}

/// The bottom input panel: a nested border framing an anti-aliased text line.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct InputConfig {
    /// Fraction of the window height the panel occupies (`0.0..=1.0`).
    pub height_fraction: f32,
    /// Outer margin from the panel edge to the first border, in device pixels.
    pub margin_px: u32,
    /// Inner padding from the innermost border to the text, in device pixels.
    pub padding_px: u32,
    /// Text caret width, in device pixels.
    pub caret_width_px: u32,
    pub background_color: Color,
    pub text_color: Color,
    /// Colour of the fixed prompt drawn before the editable answer. Defaults to
    /// the accent (a tinted label), distinct from the answer's `text_color`.
    pub prompt_color: Color,
    pub border: BorderConfig,
    pub font: FontConfig,
}

impl Default for InputConfig {
    fn default() -> Self {
        let theme = Theme::default();
        Self {
            height_fraction: 0.15,
            margin_px: 8,
            padding_px: 8,
            caret_width_px: 2,
            background_color: theme.panel,
            text_color: theme.ink,
            prompt_color: theme.accent, // tinted label, distinct from the answer
            border: BorderConfig::default(),
            font: FontConfig::default(),
        }
    }
}

/// A nested (concentric) line border.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BorderConfig {
    pub color: Color,
    /// Thickness of each line, in device pixels.
    pub line_thickness_px: u32,
    /// Number of concentric lines ("2 lines all around").
    pub line_count: u32,
    /// Gap between adjacent lines, in device pixels.
    pub line_gap_px: u32,
}

impl Default for BorderConfig {
    fn default() -> Self {
        Self {
            color: Theme::default().accent,
            line_thickness_px: 2,
            line_count: 2,
            line_gap_px: 3,
        }
    }
}

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
    /// A monospace family resolved from the OS font database.
    System { family: String },
    /// A `.ttf`/`.ttc` at an explicit path.
    File { path: PathBuf },
    /// A font bundled into the binary (none is bundled yet).
    Embedded,
}

impl Default for FontSource {
    fn default() -> Self {
        FontSource::System {
            family: "Menlo".to_string(),
        }
    }
}

/// The math-quiz example's tunables: the question, the accepted answer, and the
/// three retro banners it shows. Banner *visuals* are [`BannerConfig`]s so a
/// future level can restyle them without touching game logic; the win banner
/// reuses the [`MarqueeConfig`] palette/speed, so only its text lives here.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct QuizConfig {
    /// Prompt shown inside the input field, ahead of the caret.
    pub question: String,
    /// The answer that wins; compared numerically when both sides parse.
    pub expected: String,
    /// Frames the game-over sign lingers before returning to the question.
    pub game_over_frames: u32,
    /// Text of the winning marquee (colours/speed come from [`MarqueeConfig`]).
    pub win_text: String,
    /// The red "X" flashed on a wrong answer.
    pub cross: BannerConfig,
    /// How the cross blinks.
    pub flash: FlashConfig,
    /// The "GAME OVER" sign shown after the flashes, before the retry.
    pub game_over: BannerConfig,
}

impl Default for QuizConfig {
    fn default() -> Self {
        let theme = Theme::default();
        Self {
            question: "What is 6+6? ".to_string(),
            expected: "12".to_string(),
            game_over_frames: 90,
            win_text: "YOU WIN".to_string(),
            cross: BannerConfig {
                text: "X".to_string(),
                scale: 14,
                tracking: 0,
                shadow_depth: 0, // a flat cross: red fill + black outline, no 3D
                outline_px: 1,
                gap: 0,
                colors: TextColors {
                    fill: theme.danger,
                    outline: theme.outline,
                    shadow: theme.outline, // unused at depth 0
                },
            },
            flash: FlashConfig::default(),
            game_over: BannerConfig {
                text: "GAME OVER".to_string(),
                scale: 3, // "full sized": ~243px wide across the 256px screen
                tracking: 1,
                shadow_depth: 3,
                outline_px: 1,
                gap: 0,
                colors: TextColors {
                    fill: theme.warning,
                    outline: theme.outline,
                    shadow: theme.shadow, // gold 3D extrusion
                },
            },
        }
    }
}

/// A styled block of oversized pixel-art text, baked to a [`Sprite`] on demand.
/// The static counterpart to [`MarqueeConfig`]: the same big-text knobs, minus
/// the scroll speed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BannerConfig {
    pub text: String,
    pub scale: u32,
    pub tracking: u32,
    pub shadow_depth: u32,
    /// Outline thickness around the letters, in source pixels (`0` = none).
    pub outline_px: u32,
    pub gap: u32,
    pub colors: TextColors,
}

impl Default for BannerConfig {
    /// A neutral, empty banner — a fallback for partial config, not a banner you
    /// would show as-is. The real banners are built in [`QuizConfig::default`].
    fn default() -> Self {
        Self {
            text: String::new(),
            scale: 1,
            tracking: 1,
            shadow_depth: 0,
            outline_px: 1,
            gap: 0,
            colors: TextColors::default(),
        }
    }
}

impl BannerConfig {
    /// Bake the configured text into a sprite via [`BigText`].
    #[must_use]
    pub fn sprite(&self) -> Sprite {
        BigText::new(self.scale)
            .tracking(self.tracking)
            .shadow_depth(self.shadow_depth)
            .outline(self.outline_px)
            .gap(self.gap)
            .colors(self.colors)
            .build(&self.text)
    }
}

/// Blink timing for the reject cross: `count` on/off cycles, each showing the
/// cross for `on_frames` then hiding it for `off_frames`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FlashConfig {
    pub count: u32,
    pub on_frames: u32,
    pub off_frames: u32,
}

impl Default for FlashConfig {
    fn default() -> Self {
        Self {
            count: 3,
            on_frames: 10,
            off_frames: 8,
        }
    }
}

impl FlashConfig {
    /// Total frames one full flash sequence spans.
    #[must_use]
    pub fn total_frames(self) -> u32 {
        self.count * (self.on_frames + self.off_frames)
    }

    /// Whether the cross is visible on frame `frame` of the sequence.
    #[must_use]
    pub fn visible_at(self, frame: u32) -> bool {
        let cycle = self.on_frames + self.off_frames;
        if cycle == 0 {
            return false;
        }
        frame % cycle < self.on_frames
    }
}

/// Resolved geometry of the input panel, all derived from [`InputConfig`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputLayout {
    pub panel: Rect,
    /// Concentric border rects, outermost first.
    pub borders: Vec<Rect>,
    /// The rect glyphs are drawn (and clipped) into.
    pub text_area: Rect,
}

impl InputConfig {
    /// Compute the panel, border, and text rects for a given window size.
    /// Pure and literal-free — everything comes from `self`.
    #[must_use]
    pub fn layout(&self, window: Size) -> InputLayout {
        let panel_h = (f64::from(window.h) * f64::from(self.height_fraction)).round() as u32;
        let panel = Rect::new(
            Point::new(0, window.h.saturating_sub(panel_h) as i32),
            Size::new(window.w, panel_h),
        );

        let step = self.border.line_thickness_px + self.border.line_gap_px;
        let mut borders = Vec::with_capacity(self.border.line_count as usize);
        let mut inset = self.margin_px;
        for _ in 0..self.border.line_count {
            borders.push(inset_rect(panel, inset));
            inset += step;
        }

        // Text sits inside the innermost line: drop the trailing gap, add padding.
        let text_inset = inset.saturating_sub(self.border.line_gap_px) + self.padding_px;
        InputLayout {
            panel,
            borders,
            text_area: inset_rect(panel, text_inset),
        }
    }
}

/// Shrink a rect inward by `by` pixels on every side.
fn inset_rect(r: Rect, by: u32) -> Rect {
    let d = by as i32;
    Rect::new(
        Point::new(r.origin.x + d, r.origin.y + d),
        Size::new(r.size.w.saturating_sub(2 * by), r.size.h.saturating_sub(2 * by)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_sits_in_the_bottom_fraction() {
        let cfg = InputConfig::default();
        let l = cfg.layout(Size::new(1000, 1000));
        assert_eq!(l.panel.size, Size::new(1000, 150)); // 15%
        assert_eq!(l.panel.origin, Point::new(0, 850));
    }

    #[test]
    fn nested_borders_match_configured_count_and_nest_inward() {
        let cfg = InputConfig::default();
        let l = cfg.layout(Size::new(800, 600));
        assert_eq!(l.borders.len(), cfg.border.line_count as usize);
        // Each successive border is strictly inside the previous.
        for pair in l.borders.windows(2) {
            assert!(pair[1].origin.x > pair[0].origin.x);
            assert!(pair[1].size.w < pair[0].size.w);
        }
        // Text area is inside the innermost border.
        let inner = l.borders.last().unwrap();
        assert!(l.text_area.origin.x >= inner.origin.x);
    }

    #[test]
    fn defaults_hold_no_magic_numbers_downstream() {
        // The one place 20px / 0.15 / light-blue live is here.
        let cfg = InputConfig::default();
        assert!((cfg.font.size_px - 20.0).abs() < f32::EPSILON);
        assert!((cfg.height_fraction - 0.15).abs() < f32::EPSILON);
        assert_eq!(cfg.border.line_count, 2);
    }

    #[test]
    fn quiz_defaults_are_playable() {
        let q = QuizConfig::default();
        assert_eq!(q.expected, "12");
        assert_eq!(q.flash.count, 3);
        // The cross bakes to a non-empty sprite.
        assert!(q.cross.sprite().size().area() > 0);
    }

    #[test]
    fn game_over_banner_is_full_sized_but_fits_the_screen() {
        let q = QuizConfig::default();
        let screen = ScreenConfig::default().size;
        let banner = q.game_over.sprite().size();
        // "Full sized": spans most of the screen width without overflowing it.
        assert!(banner.w <= screen.w, "banner must fit the virtual screen");
        assert!(banner.w > screen.w / 2, "banner should read as full sized");
    }

    #[test]
    fn flash_visibility_toggles_within_a_cycle() {
        let f = FlashConfig::default();
        assert!(f.visible_at(0)); // on at the start of a cycle
        assert!(!f.visible_at(f.on_frames)); // off once past the on window
        assert!(f.visible_at(f.on_frames + f.off_frames)); // on again next cycle
        assert_eq!(f.total_frames(), f.count * (f.on_frames + f.off_frames));
    }

    #[test]
    fn defaults_round_trip_through_toml() {
        let cfg = Config::default();
        let text = toml::to_string(&cfg).expect("serialize");
        let parsed: Config = toml::from_str(&text).expect("deserialize");
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn a_partial_file_falls_back_to_defaults() {
        let parsed: Config = toml::from_str("[window]\ntitle = \"custom\"\n").expect("parse");
        assert_eq!(parsed.window.title, "custom");
        assert_eq!(parsed.window.width, WindowConfig::default().width);
        assert_eq!(parsed.screen.size, ScreenConfig::default().size);
        assert_eq!(parsed.quiz.expected, QuizConfig::default().expected);
    }

    #[test]
    fn component_colours_default_from_the_theme() {
        let theme = Theme::default();
        let cfg = Config::default();
        assert_eq!(cfg.screen.backdrop, theme.background);
        assert_eq!(cfg.input.border.color, theme.accent);
        assert_eq!(cfg.input.text_color, theme.ink);
        assert_eq!(cfg.input.background_color, theme.panel);
        assert_eq!(cfg.quiz.cross.colors.fill, theme.danger);
        assert_eq!(cfg.quiz.game_over.colors.fill, theme.warning);
    }

    #[test]
    fn a_colour_can_be_overridden_in_toml() {
        let parsed: Config =
            toml::from_str("[input.border]\ncolor = \"#FF0000\"\n").expect("parse");
        assert_eq!(parsed.input.border.color, Color::rgb(0xFF, 0, 0));
        // A colour the file left alone keeps its theme default.
        assert_eq!(parsed.input.text_color, Theme::default().ink);
    }

    #[test]
    fn validate_rejects_out_of_range_height_fraction() {
        let mut cfg = Config::default();
        cfg.input.height_fraction = 1.5;
        assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));
        cfg.input.height_fraction = 0.0;
        assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));
        cfg.input.height_fraction = 0.15;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_rejects_zero_dimensions() {
        let mut cfg = Config::default();
        cfg.screen.size = Size::new(0, 240);
        assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn load_missing_file_is_an_io_error() {
        let err = Config::load("/no/such/ratgames-config.toml").unwrap_err();
        assert!(matches!(err, ConfigError::Io { .. }));
    }
}

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
use crate::font::{FontError, SystemFont};
use crate::geometry::{Point, Rect, Size};
use crate::glyph::{Bitmap8x8, GlyphSource, RasterGlyphSource};
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
fn validate_glyph_source(source: &GlyphSourceConfig, ctx: &str) -> Result<(), ConfigError> {
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
fn guard_footprint(text: &BigText, source: &dyn GlyphSource, s: &str) -> Result<(), ConfigError> {
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
    /// Which glyph source the letters rasterise through.
    pub glyph_source: GlyphSourceConfig,
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
            glyph_source: GlyphSourceConfig::Bitmap8x8,
        }
    }
}

impl MarqueeConfig {
    /// Bake `text` into a scrolling-banner sprite through the configured glyph
    /// source and style.
    ///
    /// # Errors
    /// Returns [`ConfigError::Font`] if a raster glyph source's font cannot be
    /// loaded, or [`ConfigError::SpriteTooLarge`] if the banner would exceed the
    /// pre-allocation size limits.
    pub fn text_sprite(&self, text: &str) -> Result<Sprite, ConfigError> {
        // Bound the glyph source at this boundary too: a banner can be baked from
        // a Config that never went through validate() (e.g. Config::default()),
        // and an oversized cell_px must be rejected before it is rasterised.
        validate_glyph_source(&self.glyph_source, "marquee")?;
        let source = self.glyph_source.resolve()?;
        let big = BigText::new(self.text_scale)
            .tracking(self.tracking)
            .shadow_depth(self.shadow_depth)
            .outline(self.outline_px)
            .gap(self.gap)
            .colors(self.colors);
        guard_footprint(&big, &*source, text)?;
        Ok(big.build_with(&*source, text))
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
                glyph_source: GlyphSourceConfig::Bitmap8x8,
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
                glyph_source: GlyphSourceConfig::Bitmap8x8,
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
    /// Which glyph source the letters rasterise through.
    pub glyph_source: GlyphSourceConfig,
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
            glyph_source: GlyphSourceConfig::Bitmap8x8,
        }
    }
}

impl BannerConfig {
    /// Bake the configured text into a sprite via [`BigText`], through the
    /// configured glyph source.
    ///
    /// # Errors
    /// Returns [`ConfigError::Font`] if a raster glyph source's font cannot be
    /// loaded, or [`ConfigError::SpriteTooLarge`] if the banner would exceed the
    /// pre-allocation size limits.
    pub fn sprite(&self) -> Result<Sprite, ConfigError> {
        // See MarqueeConfig::text_sprite: validate the source here too, so an
        // unvalidated Config cannot rasterise a runaway cell_px.
        validate_glyph_source(&self.glyph_source, "banner")?;
        let source = self.glyph_source.resolve()?;
        let big = BigText::new(self.scale)
            .tracking(self.tracking)
            .shadow_depth(self.shadow_depth)
            .outline(self.outline_px)
            .gap(self.gap)
            .colors(self.colors);
        guard_footprint(&big, &*source, &self.text)?;
        Ok(big.build_with(&*source, &self.text))
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
        Size::new(
            r.size.w.saturating_sub(2 * by),
            r.size.h.saturating_sub(2 * by),
        ),
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
        assert!(q.cross.sprite().expect("bitmap source").size().area() > 0);
    }

    #[test]
    fn game_over_banner_is_full_sized_but_fits_the_screen() {
        let q = QuizConfig::default();
        let screen = ScreenConfig::default().size;
        let banner = q.game_over.sprite().expect("bitmap source").size();
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

    #[test]
    fn raster_glyph_source_round_trips_through_toml() {
        let raster = GlyphSourceConfig::Raster {
            cell_px: 24,
            threshold: 128,
            font: FontSource::System {
                family: "Menlo".to_string(),
            },
        };
        let banner = BannerConfig {
            glyph_source: raster.clone(),
            ..BannerConfig::default()
        };
        let text = toml::to_string(&banner).expect("serialize");
        let parsed: BannerConfig = toml::from_str(&text).expect("deserialize");
        assert_eq!(parsed.glyph_source, raster);
    }

    #[test]
    fn raster_threshold_defaults_to_128_when_omitted() {
        // A raster source declared without an explicit threshold falls back to
        // 128, so existing configs keep the current look.
        let parsed: GlyphSourceConfig = toml::from_str(
            "kind = \"raster\"\ncell_px = 24\n[font]\nkind = \"system\"\nfamily = \"Menlo\"\n",
        )
        .expect("parse");
        match parsed {
            GlyphSourceConfig::Raster { threshold, .. } => assert_eq!(threshold, 128),
            other => panic!("expected a raster source, got {other:?}"),
        }
    }

    #[test]
    fn raster_threshold_round_trips_through_toml() {
        let raster = GlyphSourceConfig::Raster {
            cell_px: 24,
            threshold: 200,
            font: FontSource::System {
                family: "Menlo".to_string(),
            },
        };
        let banner = BannerConfig {
            glyph_source: raster.clone(),
            ..BannerConfig::default()
        };
        let text = toml::to_string(&banner).expect("serialize");
        let parsed: BannerConfig = toml::from_str(&text).expect("deserialize");
        assert_eq!(parsed.glyph_source, raster);
    }

    #[test]
    fn validate_rejects_zero_text_scale() {
        let mut cfg = Config::default();
        cfg.marquee.text_scale = 0;
        assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn validate_rejects_zero_banner_scale() {
        let mut cfg = Config::default();
        cfg.quiz.cross.scale = 0;
        assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn validate_rejects_zero_raster_cell_px() {
        // On the marquee source.
        let mut cfg = Config::default();
        cfg.marquee.glyph_source = GlyphSourceConfig::Raster {
            cell_px: 0,
            threshold: 128,
            font: FontSource::default(),
        };
        assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));

        // And on a banner source.
        let mut cfg = Config::default();
        cfg.quiz.game_over.glyph_source = GlyphSourceConfig::Raster {
            cell_px: 0,
            threshold: 128,
            font: FontSource::default(),
        };
        assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn validate_rejects_oversized_raster_cell_px() {
        let mut cfg = Config::default();
        cfg.marquee.glyph_source = GlyphSourceConfig::Raster {
            cell_px: 10_000, // beyond the private safety ceiling
            threshold: 128,
            font: FontSource::default(),
        };
        assert!(matches!(cfg.validate(), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn validate_accepts_a_reasonable_raster_marquee() {
        let mut cfg = Config::default();
        cfg.marquee.glyph_source = GlyphSourceConfig::Raster {
            cell_px: 32,
            threshold: 160,
            font: FontSource::default(),
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn config_source_is_a_file_when_a_path_is_given() {
        assert_eq!(
            ConfigSource::resolve(Some(PathBuf::from("/game.toml"))),
            ConfigSource::File(PathBuf::from("/game.toml"))
        );
    }

    #[test]
    fn config_source_defaults_without_a_path() {
        assert_eq!(ConfigSource::resolve(None), ConfigSource::Default);
    }

    #[test]
    fn config_source_load_or_else_uses_the_supplied_default() {
        // The Default source defers to the caller's preset, not Config::default.
        let cfg = ConfigSource::Default
            .load_or_else(|| {
                let mut c = Config::default();
                c.marquee.text_scale = 9;
                c
            })
            .expect("default builds");
        assert_eq!(cfg.marquee.text_scale, 9);
    }

    #[test]
    fn config_source_default_loads_builtin_defaults() {
        let cfg = ConfigSource::Default.load().expect("defaults load");
        assert_eq!(cfg, Config::default());
    }

    #[test]
    fn parse_config_flag_extracts_path_and_keeps_positionals() {
        let (path, positionals) =
            parse_config_flag(["--config", "banner.toml", "HELLO"].map(String::from))
                .expect("parses");
        assert_eq!(path, Some(PathBuf::from("banner.toml")));
        assert_eq!(positionals, vec!["HELLO".to_string()]);
    }

    #[test]
    fn parse_config_flag_supports_equals_form() {
        let (path, positionals) =
            parse_config_flag(["--config=x.toml".to_string()]).expect("parses");
        assert_eq!(path, Some(PathBuf::from("x.toml")));
        assert!(positionals.is_empty());
    }

    #[test]
    fn parse_config_flag_without_flag_is_all_positional() {
        // Preserves `cargo run -- "GAME OVER"`: a bare positional is banner text.
        let (path, positionals) = parse_config_flag(["GAME OVER".to_string()]).expect("parses");
        assert_eq!(path, None);
        assert_eq!(positionals, vec!["GAME OVER".to_string()]);
    }

    #[test]
    fn parse_config_flag_missing_value_is_an_error() {
        let err = parse_config_flag(["--config".to_string()]).unwrap_err();
        assert!(matches!(err, ConfigError::Invalid(_)));
    }

    #[test]
    fn sample_marquee_toml_loads_and_validates() {
        // The shipped TOML sample parses, validates, and selects a raster source.
        let cfg = Config::load("examples/marquee.toml").expect("toml sample loads");
        assert!(matches!(
            cfg.marquee.glyph_source,
            GlyphSourceConfig::Raster { .. }
        ));
    }

    #[test]
    fn sample_marquee_json_loads_and_validates() {
        // The JSON sample loads via the same Config::load (dispatched by
        // extension) and selects the same raster source.
        let cfg = Config::load("examples/marquee.json").expect("json sample loads");
        assert!(matches!(
            cfg.marquee.glyph_source,
            GlyphSourceConfig::Raster { .. }
        ));
    }

    #[test]
    fn load_rejects_an_unsupported_extension() {
        // The extension is checked before the file is read, so even a missing path
        // is rejected as an unsupported format rather than an IO error.
        let err = Config::load("game.yaml").unwrap_err();
        assert!(matches!(err, ConfigError::Invalid(_)));
    }

    #[test]
    fn defaults_round_trip_through_json() {
        let cfg = Config::default();
        let text = serde_json::to_string(&cfg).expect("serialize");
        let parsed: Config = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn oversized_banner_is_rejected_before_allocation() {
        // A bitmap banner at an extreme scale exceeds the scaled-pixel ceiling
        // and is rejected without allocating — deterministic, no system font.
        let banner = BannerConfig {
            text: "GAME OVER".to_string(),
            scale: 256,
            ..BannerConfig::default()
        };
        assert!(matches!(
            banner.sprite(),
            Err(ConfigError::SpriteTooLarge { .. })
        ));
    }

    #[test]
    fn a_reasonable_banner_builds_within_limits() {
        let banner = BannerConfig {
            text: "OK".to_string(),
            scale: 4,
            ..BannerConfig::default()
        };
        assert!(banner.sprite().is_ok());
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn sample_marquee_bakes_through_the_raster_source() {
        // End-to-end: a file-selected raster source bakes a real, non-empty banner
        // (needs the sample's Menlo font).
        let cfg = Config::load("examples/marquee.toml").expect("sample loads");
        assert!(matches!(
            cfg.marquee.glyph_source,
            GlyphSourceConfig::Raster { .. }
        ));
        let sprite = cfg
            .marquee
            .text_sprite("HELLO")
            .expect("raster banner bakes");
        assert!(sprite.size().area() > 0, "raster banner is non-empty");
    }

    #[test]
    fn builder_rejects_oversized_cell_px_without_a_font() {
        // The builder validates the glyph source before resolving a font, so an
        // oversized raster cell_px is rejected deterministically — no system font
        // loaded, no giant allocation attempted.
        let banner = BannerConfig {
            text: "HI".to_string(),
            glyph_source: GlyphSourceConfig::Raster {
                cell_px: 5000,
                threshold: 128,
                font: FontSource::default(),
            },
            ..BannerConfig::default()
        };
        assert!(matches!(banner.sprite(), Err(ConfigError::Invalid(_))));
    }
}

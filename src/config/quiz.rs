//! Quiz configuration: the question, the three banners, and flash timing.

use crate::sprite::Sprite;
use crate::text::{BigText, TextColors};
use crate::theme::Theme;

use super::defaults::DEFAULT_STRINGS;
use super::{ConfigError, GlyphSourceConfig, guard_footprint, validate_glyph_source};

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
            question: DEFAULT_STRINGS.quiz.question.clone(),
            expected: DEFAULT_STRINGS.quiz.expected.clone(),
            game_over_frames: 90,
            win_text: DEFAULT_STRINGS.quiz.win_text.clone(),
            cross: BannerConfig {
                text: DEFAULT_STRINGS.quiz.cross_text.clone(),
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
                text: DEFAULT_STRINGS.quiz.game_over_text.clone(),
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

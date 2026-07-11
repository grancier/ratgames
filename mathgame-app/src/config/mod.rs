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

use mathgame_app::{Arithmetic, MathLevel};
use ratgames::{
    BlinkConfig, Color, Config, ConfigError, ContinueRules, Countdown, CountdownConfig,
    GlyphSourceConfig, RankRules, ScoresConfig, ScoringRules, ShadowConfig, load_levels_dir,
};

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

/// Per-answer feedback style. A correct answer washes the screen with
/// `correct_color` (a translucent tint that fades out); a wrong answer shows a
/// solid reject cross in `wrong_color`, magnified `cross_scale`× and blinked per
/// `cross_blink`, then the verdict. `duration_frames` is how long the verdict
/// holds. All frame counts are at the window's `target_fps`. Sourced from data,
/// like the rest of the app's look.
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
    /// The reject cross's blink pattern — a reusable `ratgames` timing config
    /// ([`BlinkConfig`]); the product value lives in the bundled JSON.
    pub cross_blink: BlinkConfig,
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
            cross_blink: BlinkConfig {
                blinks: 3,
                on_frames: 12,
                off_frames: 12,
            },
        }
    }
}

/// The per-question timer bar's colours. The gauge itself is the reusable
/// [`ratgames::MeterBar`] — only its product colours live here (like the feedback
/// colours); its on-screen rectangle is an app layout constant, not config,
/// matching the choice-list positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct TimerBarConfig {
    /// The draining fill colour — the time still on the clock.
    pub fill_color: Color,
    /// The track colour behind the fill — the drained / empty channel. A
    /// transparent colour shows the backdrop through the drained portion instead.
    pub track_color: Color,
}

impl Default for TimerBarConfig {
    fn default() -> Self {
        // Palette-derived fallbacks (amber fill over a near-black channel); the
        // bundled JSON carries the product colours.
        Self {
            fill_color: Color::rgb(0xFF, 0xE8, 0x5C),  // palette WARNING
            track_color: Color::rgb(0x0A, 0x0A, 0x14), // palette PANEL
        }
    }
}

/// Attract-mode timing: how long the title sits idle before the attract rotation
/// begins, and how long each attract card holds. The rotation cycles the
/// high-score board and a how-to card until any key wakes the title. Both are
/// reusable `ratgames` countdown configs; the shipped values live in the bundled
/// JSON. An `idle` of `0` frames (the Rust default) disables attract mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct AttractConfig {
    /// Title idle time before the rotation starts (`0` = never).
    pub idle: CountdownConfig,
    /// How long each attract card holds before rotating to the next.
    pub card: CountdownConfig,
}

impl Default for AttractConfig {
    fn default() -> Self {
        Self {
            idle: CountdownConfig { frames: 0 },
            card: CountdownConfig::default(),
        }
    }
}

impl AttractConfig {
    /// The armed title-idle countdown, or `None` when attract mode is off.
    #[must_use]
    pub fn idle_countdown(&self) -> Option<Countdown> {
        (self.idle.frames > 0).then(|| self.idle.countdown())
    }
}

/// The whole app config: the reusable engine config plus this app's text style,
/// per-answer feedback, level-interstitial timing, high-score settings, and the
/// run-wide starting lives.
///
/// The gauntlet's *levels* are not here — they are separate `level_<n>.json`
/// files (see [`resolve_levels`]), so adding a level is dropping in a file. This
/// config holds only what is run-wide.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Window, screen, theme, and the anti-aliased input font.
    pub engine: Config,
    /// Pixel-art banner / HUD style.
    pub text: TextStyle,
    /// The glyph source every pixel-art banner (and the reject cross) renders
    /// through — a 32px Menlo raster in the shipped config, resolved once at
    /// startup. The Rust `Default` is the neutral 8×8 bitmap; the product look
    /// comes from the bundled JSON.
    pub banner_glyphs: GlyphSourceConfig,
    /// Correct / wrong answer feedback colours and timing.
    pub feedback: FeedbackConfig,
    /// The per-question timer bar's colours (its on-screen rect is an app layout
    /// constant; the gauge is the reusable `ratgames::MeterBar`).
    pub timer_bar: TimerBarConfig,
    /// Level Intro / Level Clear screen hold timing — a reusable `ratgames`
    /// countdown config; the product value lives in the bundled JSON.
    pub interstitial: CountdownConfig,
    /// High-score board capacity and save file.
    pub scores: ScoresConfig,
    /// Run-wide starting lives. The per-level rules — clear/fail goal, reward, and
    /// input mode — live in the level files, not here.
    pub starting_lives: u32,
    /// Points awarded per whole second left on the clock when a question is
    /// answered correctly (`0` = no time bonus). The per-level time limit itself is
    /// authored in the level files (`LevelSpec::time_limit_frames`).
    pub time_bonus_per_second: u32,
    /// Run-wide arcade scoring: the combo bonus, perfect-clear bonus, and 1UP
    /// thresholds with a lives cap. A reusable `ratgames` rules type; the product
    /// values live in the bundled JSON.
    pub scoring: ScoringRules,
    /// Rank-based endings, proudest first — the result screen shows the first
    /// rank a finished run earns instead of the plain win / game-over title. A
    /// reusable `ratgames` rules type; the product titles live in the bundled
    /// JSON. Empty (the Rust default) keeps the plain titles.
    pub ranks: RankRules,
    /// The arcade continue policy: how many continues a run may use and whether
    /// the score survives one. A reusable `ratgames` rules type; the product
    /// values live in the bundled JSON. The Rust default offers none.
    pub continues: ContinueRules,
    /// How long the game-over CONTINUE? prompt holds before declining — a
    /// reusable `ratgames` countdown config; the product value lives in the
    /// bundled JSON.
    pub continue_prompt: CountdownConfig,
    /// Attract-mode timing: the title's idle trigger and the per-card hold. The
    /// Rust default leaves attract mode off; the shipped values turn it on.
    pub attract: AttractConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        // A playable neutral default: three lives, like an arcade run. The named
        // faces and product look still come from the bundled JSON, not here.
        Self {
            engine: Config::default(),
            text: TextStyle::default(),
            banner_glyphs: GlyphSourceConfig::default(),
            feedback: FeedbackConfig::default(),
            timer_bar: TimerBarConfig::default(),
            interstitial: CountdownConfig::default(),
            scores: ScoresConfig::default(),
            starting_lives: 3,
            time_bonus_per_second: 10,
            scoring: ScoringRules::default(),
            ranks: RankRules::default(),
            continues: ContinueRules::default(),
            continue_prompt: CountdownConfig::default(),
            attract: AttractConfig::default(),
        }
    }
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
    #[error(transparent)]
    Levels(#[from] ratgames::LevelLoadError),
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
        if self.feedback.cross_blink.blinks == 0 {
            return Err(AppConfigError::Invalid(
                "feedback.cross_blink.blinks must be at least 1".to_string(),
            ));
        }
        if self.feedback.cross_blink.on_frames == 0 {
            return Err(AppConfigError::Invalid(
                "feedback.cross_blink.on_frames must be at least 1".to_string(),
            ));
        }
        if self.starting_lives == 0 {
            return Err(AppConfigError::Invalid(
                "starting_lives must be at least 1".to_string(),
            ));
        }
        // Intra-scoring invariants (ascending, non-zero 1UP thresholds). The
        // lives-cap-vs-starting-lives cross-check needs the run and is enforced
        // when the session applies the rules (`GameRun::set_scoring`).
        self.scoring
            .validate()
            .map_err(|e| AppConfigError::Invalid(format!("scoring: {e}")))?;
        self.ranks
            .validate()
            .map_err(|e| AppConfigError::Invalid(format!("ranks: {e}")))?;
        if self.continues.allowed > 0 && self.continue_prompt.frames == 0 {
            return Err(AppConfigError::Invalid(
                "continue_prompt.frames must be at least 1 when continues are offered".to_string(),
            ));
        }
        if self.attract.idle.frames > 0 && self.attract.card.frames == 0 {
            return Err(AppConfigError::Invalid(
                "attract.card.frames must be at least 1 when attract mode is on".to_string(),
            ));
        }
        self.engine.validate()?;
        Ok(())
    }
}

/// The bundled default gauntlet, embedded at compile time and parsed once — one
/// `level_<n>.json` per level, in order. A malformed bundle is caught by the unit
/// test below, not left as a runtime risk.
static BUNDLED_LEVELS: LazyLock<Vec<MathLevel>> = LazyLock::new(|| {
    const FILES: &[&str] = &[
        include_str!("levels/level_0.json"),
        include_str!("levels/level_1.json"),
        include_str!("levels/level_2.json"),
        include_str!("levels/level_3.json"),
    ];
    FILES
        .iter()
        .map(|text| {
            serde_json::from_str(text).expect("bundled config/levels/level_<n>.json must be valid")
        })
        .collect()
});

/// The levels for this run, in order: the `--levels <dir>` directory's
/// `level_<n>.json` files (sorted by index) if given, else the bundled gauntlet.
///
/// Level *content* is validated later, when the session builds the campaign from
/// these (bad operand ranges, unplayable goals); this only reads and parses.
///
/// # Errors
/// [`AppConfigError`] if the directory cannot be read, holds no `level_<n>.json`
/// files, or a file cannot be read or parsed.
pub fn resolve_levels(cli_dir: Option<PathBuf>) -> Result<Vec<MathLevel>, AppConfigError> {
    match cli_dir {
        Some(dir) => Ok(load_levels_dir::<Arithmetic>(&dir)?),
        None => Ok(BUNDLED_LEVELS.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mathgame_app::{MathgameSession, OperatorConfig};
    use ratgames::{AnswerMode, FontFamily, FontSource, FontWeight, Size};

    #[test]
    fn bundled_default_selects_the_product_structure() {
        // The bundled JSON is the source of truth for the product look, not a Rust
        // literal. Pin only the structural choices — the faces (Menlo Bold, input
        // and banners), the screen geometry, the banner glyph source and its
        // resolution, where scores persist. The continuous scalars (scales,
        // offsets, sizes, thresholds, colours, frame counts) are live-tuned in
        // defaults.json while the window runs; exact-value pins broke this test on
        // every tuning pass, and `resolve()` already validates their invariants.
        let config = AppConfig::resolve(None).expect("bundled config must be valid");
        match config.engine.input.font.source {
            FontSource::System {
                family: FontFamily::Named(name),
                weight,
                ..
            } => {
                assert_eq!(name, "Menlo");
                assert_eq!(weight, FontWeight(700));
            }
            other => panic!("expected a named system font, got {other:?}"),
        }
        assert_eq!(config.engine.screen.size, Size::new(640, 360));
        match &config.banner_glyphs {
            GlyphSourceConfig::Raster { cell_px, font, .. } => {
                assert_eq!(*cell_px, 32);
                match font {
                    FontSource::System {
                        family: FontFamily::Named(name),
                        weight,
                        ..
                    } => {
                        assert_eq!(name, "Menlo");
                        assert_eq!(*weight, FontWeight(700));
                    }
                    other => panic!("expected a Menlo raster font, got {other:?}"),
                }
            }
            other => panic!("expected a 32px raster banner source, got {other:?}"),
        }
        assert_eq!(config.scores.capacity, 10);
        assert_eq!(
            config.scores.file,
            std::path::PathBuf::from("mathgame-highscores.json")
        );
        // Run-wide starting lives are shipped game *design* — not a live-tuned
        // visual knob like the scales/offsets/colours above — so pin them. The
        // per-level rules live in the level files, pinned by the test below.
        assert_eq!(config.starting_lives, 3);
    }

    #[test]
    fn bundled_ranks_name_the_shipped_endings() {
        // The rank table is shipped game design: pin its shape — the two win
        // endings, proudest first, every rule win-gated so a game over keeps its
        // plain title. The failure/point thresholds stay tunable.
        let config = AppConfig::resolve(None).expect("bundled config");
        let titles: Vec<_> = config
            .ranks
            .rules
            .iter()
            .map(|rule| rule.title.as_str())
            .collect();
        assert_eq!(titles, vec!["NO MISS CHAMP", "MATH MASTER"]);
        assert!(
            config.ranks.rules.iter().all(|rule| rule.requires_won),
            "a lost run keeps the plain GAME OVER title"
        );
    }

    #[test]
    fn bundled_continues_offer_one_score_keeping_continue() {
        // Shipped game design: one continue that keeps the score, prompted for
        // ten seconds at 60fps. The counts stay tunable; pin that a continue is
        // offered and the prompt has a real hold.
        let config = AppConfig::resolve(None).expect("bundled config");
        assert!(config.continues.allowed >= 1);
        assert!(config.continue_prompt.frames >= 1);
    }

    #[test]
    fn bundled_attract_mode_is_on_with_a_real_rotation() {
        // Shipped game design: the title idles into an attract rotation. The
        // frame counts stay tunable; pin that both holds are real.
        let config = AppConfig::resolve(None).expect("bundled config");
        assert!(config.attract.idle.frames > 0, "attract mode is on");
        assert!(config.attract.card.frames > 0);
        assert!(config.attract.idle_countdown().is_some());
    }

    #[test]
    fn attract_mode_defaults_off_and_rejects_a_rotation_with_no_hold() {
        // The Rust default keeps attract off (no idle trigger)...
        let config = AppConfig::default();
        assert!(config.attract.idle_countdown().is_none());
        assert!(config.validate().is_ok());

        // ...and turning it on with a zero card hold is a config error (the
        // rotation would thrash every frame).
        let broken = AppConfig {
            attract: AttractConfig {
                idle: CountdownConfig { frames: 600 },
                card: CountdownConfig { frames: 0 },
            },
            ..AppConfig::default()
        };
        assert!(matches!(broken.validate(), Err(AppConfigError::Invalid(_))));
    }

    #[test]
    fn bundled_levels_form_the_shipped_gauntlet() {
        // The gauntlet is shipped game design: an ordered, four-operator run.
        // Pin its shape (count, order, operators, input mode) — the tunable
        // ranges/points/labels are free to change in the level files.
        let levels = resolve_levels(None).expect("bundled levels must be valid");
        let operators: Vec<_> = levels.iter().map(|level| level.content.operator).collect();
        assert_eq!(
            operators,
            vec![
                OperatorConfig::Add,
                OperatorConfig::Subtract,
                OperatorConfig::Multiply,
                OperatorConfig::Divide,
            ]
        );
        assert_eq!(levels[0].name, "NUMBER YARD");
        // The shipped play modes: the opening level grades typed answers; the
        // rest are arcade multiple choice with four options.
        assert_eq!(levels[0].rules.answer_mode, AnswerMode::Typed);
        for level in &levels[1..] {
            assert_eq!(
                level.rules.answer_mode,
                AnswerMode::MultipleChoice { options: 4 }
            );
        }
        // The whole gauntlet builds a playable session.
        assert!(MathgameSession::from_levels(&levels, config_starting_lives(), 1).is_ok());
    }

    #[test]
    fn bundled_scoring_is_valid_and_applies_to_the_shipped_run() {
        // The shipped scoring is game design, but its values are a first cut to be
        // play-tuned — so pin only that it is present, well-formed, and applies
        // cleanly to the shipped run (its lives cap is not below the starting
        // lives). `resolve` already validates the intra-scoring invariants.
        let config = AppConfig::resolve(None).expect("bundled config");
        assert!(
            !config.scoring.one_up.thresholds.is_empty(),
            "the shipped gauntlet configures 1UP thresholds"
        );
        let levels = resolve_levels(None).expect("bundled levels");
        assert!(
            MathgameSession::from_levels(&levels, config.starting_lives, 1)
                .and_then(|session| session.with_scoring(config.scoring.clone()))
                .is_ok(),
            "bundled scoring must apply cleanly to the shipped run"
        );
    }

    /// The bundled run-wide starting lives, for tests that build a session.
    fn config_starting_lives() -> u32 {
        AppConfig::resolve(None)
            .expect("bundled config")
            .starting_lives
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

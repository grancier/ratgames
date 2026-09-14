//! The app's configuration: `ratgames::Config` (the engine) and the reusable
//! ratgames widget configs, plus this app's own copy / layout / difficulty
//! types — sourced from data, not hardcoded in Rust.
//!
//! The default lives in bundled per-domain JSON files — `engine.json`,
//! `style.json`, `economy.json`, `profiles.json`, `copy.json`, `layout.json` — embedded at
//! compile time and parsed once, so `cargo run -p mathgame-app` needs no
//! external file yet no product value — the Menlo input font, its size, the
//! banner/HUD scale and shadow depth — is baked into a Rust literal. A
//! `--config <path>` flag overrides it with a single TOML or JSON file (chosen
//! by extension), exactly like the ratgames examples. Rust holds only the
//! config *types* and their `Default` fallbacks, never the product choices
//! themselves.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod profiles_tests;

mod bundled;
mod copy;
mod layout;
mod levels;
mod profiles;

use bundled::BUNDLED;
pub use copy::{CopyConfig, ResultCopy, VerdictCopy};
pub use layout::LayoutConfig;
pub use levels::resolve_levels;
pub use profiles::{AuthoredLevel, DifficultyProfile, PreparedDifficulty};

use ratgames::{
    AttractConfig, BannerStyle, Config, ConfigError, ConfigFileError, ContinueRules,
    CountdownConfig, FeedbackBeatConfig, GlyphSourceConfig, MeterBarConfig, RankRules,
    ScoresConfig, ScoringRules, load_config_file,
};

/// One selectable difficulty: its menu label and the run knobs it turns. A
/// preset can select each level's mapped problem profile, starts the run with
/// its own lives, and scales every level's authored
/// time limit by `time_percent` (100 = as authored; an untimed level stays
/// untimed). The presets are product values in the bundled JSON; an empty list
/// (the Rust default) skips the difficulty-select screen entirely.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DifficultyPreset {
    /// Stable mapping key, independent of the editable label. Omitted selects
    /// the legacy inline level content and only adjusts lives/time.
    #[serde(default)]
    pub id: Option<String>,
    /// The menu label (e.g. `"NORMAL"`).
    pub label: String,
    /// Run-wide starting lives under this difficulty.
    pub starting_lives: u32,
    /// Percent applied to every level's `time_limit_frames` (100 = as authored,
    /// more = easier). Defaults to 100 when omitted.
    #[serde(default = "default_time_percent")]
    pub time_percent: u32,
}

fn default_time_percent() -> u32 {
    100
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
    pub text: BannerStyle,
    /// The glyph source the display-height banners (titles, verdicts, the
    /// equation — the `banner_scale` family) and the reject cross render
    /// through — a 64px Menlo raster in the shipped config, resolved once at
    /// startup. The Rust `Default` is the neutral 8×8 bitmap; the product look
    /// comes from the bundled JSON.
    pub banner_glyphs: GlyphSourceConfig,
    /// The glyph source for body-height text (the HUD line, choice lists,
    /// board rows, readouts — the `hud_scale` family). `None` (the Rust
    /// default) shares `banner_glyphs`; the shipped config sets a smaller
    /// raster (32px Menlo) so both text sizes render at the full resolution
    /// their height allows — pixel height is `cell_px × scale` and the crisp
    /// pipeline never downsamples, so each size needs a source rasterised at
    /// its own height.
    pub hud_glyphs: Option<GlyphSourceConfig>,
    /// Correct / wrong answer feedback colours and timing.
    pub feedback: FeedbackBeatConfig,
    /// The per-question timer bar's colours (its on-screen rect is an app layout
    /// constant; the gauge is the reusable `ratgames::MeterBar`).
    pub timer_bar: MeterBarConfig,
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
    /// The selectable difficulties, in menu order. Empty (the Rust default)
    /// skips the select screen and plays the gauntlet exactly as authored, with
    /// the run-wide `starting_lives` above.
    pub difficulties: Vec<DifficultyPreset>,
    /// Authored problem mixes, referenced by level `profiles_by_mode` mappings.
    pub difficulty_profiles: BTreeMap<String, DifficultyProfile>,
    /// Every user-facing string. Blank by default; the product copy lives in the
    /// bundled `copy.json`, merged in at load.
    pub copy: CopyConfig,
    /// Where every screen element sits. Neutral by default; the product positions
    /// live in the bundled `layout.json`, merged in at load.
    pub layout: LayoutConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        // A playable neutral default: three lives, like an arcade run. The named
        // faces and product look still come from the bundled JSON, not here.
        Self {
            engine: Config::default(),
            text: BannerStyle::default(),
            banner_glyphs: GlyphSourceConfig::default(),
            hud_glyphs: None,
            feedback: FeedbackBeatConfig::default(),
            timer_bar: MeterBarConfig::default(),
            interstitial: CountdownConfig::default(),
            scores: ScoresConfig::default(),
            starting_lives: 3,
            time_bonus_per_second: 10,
            scoring: ScoringRules::default(),
            ranks: RankRules::default(),
            continues: ContinueRules::default(),
            continue_prompt: CountdownConfig::default(),
            attract: AttractConfig::default(),
            difficulties: Vec::new(),
            difficulty_profiles: BTreeMap::new(),
            copy: CopyConfig::default(),
            layout: LayoutConfig::default(),
        }
    }
}

/// Errors materialising an [`AppConfig`].
#[derive(Debug, thiserror::Error)]
pub enum AppConfigError {
    /// The `--config` file could not be read or parsed — the shared
    /// [`ratgames::load_config_file`] failure, reported verbatim.
    #[error(transparent)]
    File(#[from] ConfigFileError),
    #[error("invalid config: {0}")]
    Invalid(String),
    #[error(transparent)]
    Engine(#[from] ConfigError),
    #[error(transparent)]
    Levels(#[from] ratgames::LevelLoadError),
}

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

    /// Read and parse a config file, choosing TOML or JSON by its extension —
    /// the shared [`ratgames::load_config_file`] loader; only the target type
    /// is this app's.
    fn load_file(path: &Path) -> Result<Self, AppConfigError> {
        Ok(load_config_file(path)?)
    }

    /// The app's own invariants plus the engine's and the reusable widgets'.
    /// Each component checks itself (`Config::validate` covers the window /
    /// screen / input font; the ratgames widget configs cover their own
    /// scales, offsets, timings, and files); here we keep only the app
    /// composition — starting lives — and the checks that span components.
    fn validate(&self) -> Result<(), AppConfigError> {
        self.validate_profiles()?;
        self.text
            .validate()
            .map_err(|e| AppConfigError::Invalid(format!("text: {e}")))?;
        self.scores
            .validate()
            .map_err(|e| AppConfigError::Invalid(format!("scores: {e}")))?;
        self.feedback
            .validate()
            .map_err(|e| AppConfigError::Invalid(format!("feedback: {e}")))?;
        self.attract
            .validate()
            .map_err(|e| AppConfigError::Invalid(format!("attract: {e}")))?;
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
        for (index, preset) in self.difficulties.iter().enumerate() {
            if preset.label.is_empty() {
                return Err(AppConfigError::Invalid(format!(
                    "difficulties[{index}]: the label must not be empty"
                )));
            }
            if preset.starting_lives == 0 {
                return Err(AppConfigError::Invalid(format!(
                    "difficulties[{index}] ({}): starting_lives must be at least 1",
                    preset.label
                )));
            }
            if preset.time_percent == 0 {
                return Err(AppConfigError::Invalid(format!(
                    "difficulties[{index}] ({}): time_percent must be at least 1",
                    preset.label
                )));
            }
            // The same cross-check the session applies at startup: a preset the
            // scoring lives cap forbids would fail mid-flow at select time, so
            // catch it here where the whole config is in view.
            if self.scoring.one_up.max_lives < preset.starting_lives {
                return Err(AppConfigError::Invalid(format!(
                    "difficulties[{index}] ({}): starting_lives exceeds scoring.one_up.max_lives",
                    preset.label
                )));
            }
        }
        self.engine.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

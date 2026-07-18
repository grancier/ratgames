//! The app's configuration: `ratgames::Config` (the engine) and the reusable
//! ratgames widget configs, plus this app's own copy / layout / difficulty
//! types — sourced from data, not hardcoded in Rust.
//!
//! The default lives in bundled per-domain JSON files — `engine.json`,
//! `style.json`, `economy.json`, `copy.json`, `layout.json` — embedded at
//! compile time and parsed once, so `cargo run -p wordgame-app` needs no
//! external file yet no product value — the Menlo input font, its size, the
//! banner/HUD scale and shadow depth — is baked into a Rust literal. A
//! `--config <path>` flag overrides it with a single TOML or JSON file (chosen
//! by extension), exactly like the ratgames examples. Rust holds only the
//! config *types* and their `Default` fallbacks, never the product choices
//! themselves.
//!
//! The gauntlet's levels and the word pool are likewise data: `levels/
//! level_<n>.json` (or a `--levels <dir>` override) and `words.json`.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use ratgames::{
    AttractConfig, BannerStyle, Config, ConfigError, ContinueRules, CountdownConfig,
    FeedbackBeatConfig, GlyphSourceConfig, HighScoreLayout, MeterBarConfig, Point, RankRules, Rect,
    ScoresConfig, ScoringRules, Size, load_levels_dir,
};
use wordgame_app::{WordLevel, Words};
use wordgame_core::WordList;

/// One selectable difficulty: its menu label and the run knobs it turns. A
/// preset starts the run with its own lives and scales every level's authored
/// time limit by `time_percent` (100 = as authored; an untimed level stays
/// untimed). The presets are product values in the bundled JSON; an empty list
/// (the Rust default) skips the difficulty-select screen entirely.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DifficultyPreset {
    /// The menu label (e.g. `"NORMAL"`).
    pub label: String,
    /// Run-wide starting lives under this difficulty.
    pub starting_lives: u32,
    /// Percent applied to every level's `time_limit_frames` (100 = as
    /// authored, more = easier). Defaults to 100 when omitted.
    #[serde(default = "default_time_percent")]
    pub time_percent: u32,
}

fn default_time_percent() -> u32 {
    100
}

/// All user-facing copy — every on-screen string, sourced from JSON like the
/// rest of the app's look, never a Rust literal. Format strings hold `{}`
/// placeholders filled left-to-right by `ratgames::fill_placeholders`. The
/// [`Default`] is deliberately blank so the product copy lives only in
/// `copy.json`; the bundled config supplies it all.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct CopyConfig {
    /// Title-screen banner, e.g. `"WORD GAME"`.
    pub title: String,
    /// The name-entry input prompt, e.g. `"NAME: "`.
    pub name_prompt: String,
    /// The answer input prompt entering play, e.g. `"LETTERS: "`.
    pub answer_prompt: String,
    /// Fallback player name when name entry is left blank, e.g. `"PLAYER"`.
    pub default_player: String,
    /// Score / lives / level HUD template — three `{}` (score, lives, level).
    pub hud: String,
    /// Difficulty-select screen title, e.g. `"SELECT DIFFICULTY"`.
    pub select_difficulty: String,
    /// The attract-loop how-to card.
    pub howto: HowToCopy,
    /// Per-answer verdict text.
    pub verdict: VerdictCopy,
    /// Level-intro card lines.
    pub level_intro: LevelIntroCopy,
    /// Level-clear card lines.
    pub level_clear: LevelClearCopy,
    /// Game-over continue prompt.
    pub continue_prompt: ContinueCopy,
    /// End-of-run result screen.
    pub result: ResultCopy,
    /// High-score board header / footer.
    pub board: BoardCopy,
}

/// The attract-loop how-to card: a title over a list of instruction lines.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct HowToCopy {
    /// Card title, e.g. `"HOW TO PLAY"`.
    pub title: String,
    /// The instruction lines, top to bottom.
    pub lines: Vec<String>,
}

/// Per-answer verdict text: a hit reads `correct`; a miss reveals the word
/// (`answer_is`, one `{}`) or falls back to `wrong`; a timeout reads `time_up`.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct VerdictCopy {
    /// Correct-answer verdict, e.g. `"CORRECT"`.
    pub correct: String,
    /// Wrong-answer verdict revealing the word — one `{}`, e.g. `"THE WORD WAS {}"`.
    pub answer_is: String,
    /// Wrong-answer fallback when no word is revealed, e.g. `"WRONG"`.
    pub wrong: String,
    /// Timeout verdict, e.g. `"TIME UP"`.
    pub time_up: String,
}

/// Level-intro card: a `"ROUND {} OF {}"` line (round, total) and a `"{}  GET
/// {} RIGHT"` goal line (difficulty, required successes).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct LevelIntroCopy {
    /// Round header — two `{}` (this round, total rounds).
    pub round: String,
    /// Goal line — two `{}` (difficulty, successes needed).
    pub goal: String,
}

/// Level-clear card: a title, a `"SCORE {}"` line, and an `"ACCURACY {}%"` line.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct LevelClearCopy {
    /// Card title, e.g. `"LEVEL CLEAR"`.
    pub title: String,
    /// Running-score line — one `{}`.
    pub score: String,
    /// Accuracy line — one `{}` (a whole-number percent), e.g. `"ACCURACY {}%"`.
    pub accuracy: String,
}

/// Game-over continue prompt: a title and a `"... {} LEFT"` line (continues left).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct ContinueCopy {
    /// Prompt title, e.g. `"CONTINUE?"`.
    pub title: String,
    /// Prompt line — one `{}` (continues remaining), e.g. `"ENTER TO CONTINUE  {} LEFT"`.
    pub prompt: String,
}

/// End-of-run result screen: the win / game-over title (a configured rank shows
/// over these) and a `"SCORE {}   ENTER"` line.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct ResultCopy {
    /// Plain win title (no rank earned), e.g. `"YOU WIN"`.
    pub win: String,
    /// Plain game-over title, e.g. `"GAME OVER"`.
    pub game_over: String,
    /// Final-score line — one `{}`, e.g. `"SCORE {}   ENTER"`.
    pub score: String,
}

/// High-score board header and footer text.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct BoardCopy {
    /// Board header, e.g. `"HIGH SCORES"`.
    pub header: String,
    /// Board footer hint, e.g. `"PRESS ENTER"`.
    pub footer: String,
}

/// Where every screen element sits, in virtual-screen pixels — sourced from
/// JSON like the copy, reusing the `ratgames` geometry primitives ([`Point`],
/// [`Rect`], [`HighScoreLayout`]). The [`Default`] is neutral (origin / zero)
/// so the product positions live only in `layout.json`; the bundled config
/// supplies them. (No multiple-choice anchors here — wordgame answers are
/// typed, so the word banner always centres over the input field.)
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    /// Top-left anchor of the score / lives / level HUD line.
    pub hud_at: Point,
    /// Shared left margin (x) for the interstitial / attract / continue / menu text.
    pub screen_x: i32,
    /// Y of the how-to and difficulty-select screen titles.
    pub title_y: i32,
    /// The how-to card's instruction-line Ys, top to bottom.
    pub howto_line_ys: Vec<i32>,
    /// Difficulty menu: first-row Y (at `screen_x`) and the row pitch.
    pub menu_y: i32,
    /// Vertical spacing between difficulty menu rows.
    pub menu_row_pitch: i32,
    /// The per-question timer bar's rectangle.
    pub timer_bar: Rect,
    /// The question clock's digital seconds-readout anchor, or `None` (the
    /// neutral default) to show the draining bar alone.
    pub timer_seconds_at: Option<Point>,
    /// Level-intro card line Ys (round, level name, goal).
    pub level_intro_ys: Vec<i32>,
    /// Level-clear card line Ys (title, level name, score, accuracy).
    pub level_clear_ys: Vec<i32>,
    /// Continue-prompt subtitle Y (at `screen_x`).
    pub continue_prompt_y: i32,
    /// The continue prompt's live seconds-remaining digit anchor.
    pub continue_seconds_at: Point,
    /// Result-screen score line anchor.
    pub result_score_at: Point,
    /// The high-score board grid layout.
    pub board: HighScoreLayout,
    /// The board header anchor.
    pub board_header_at: Point,
    /// Gap below the board rows to the footer.
    pub board_footer_gap: i32,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        // Neutral: everything at the origin / zero, so an unconfigured run has
        // no product positions baked into Rust — they come from `layout.json`.
        Self {
            hud_at: Point::ORIGIN,
            screen_x: 0,
            title_y: 0,
            howto_line_ys: Vec::new(),
            menu_y: 0,
            menu_row_pitch: 0,
            timer_bar: Rect::new(Point::ORIGIN, Size::new(0, 0)),
            timer_seconds_at: None,
            level_intro_ys: Vec::new(),
            level_clear_ys: Vec::new(),
            continue_prompt_y: 0,
            continue_seconds_at: Point::ORIGIN,
            result_score_at: Point::ORIGIN,
            board: HighScoreLayout {
                origin: Point::ORIGIN,
                row_pitch: 0,
                column_width: 0,
                rows_per_column: 0,
                name_width: 0,
            },
            board_header_at: Point::ORIGIN,
            board_footer_gap: 0,
        }
    }
}

/// The whole app config: the reusable engine config plus this app's text
/// style, per-answer feedback, level-interstitial timing, high-score settings,
/// and the run-wide starting lives.
///
/// The gauntlet's *levels* and the word pool are not here — they are separate
/// data files (see [`resolve_levels`] and [`bundled_words`]), so adding a
/// level is dropping in a file. This config holds only what is run-wide.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Window, screen, theme, and the anti-aliased input font.
    pub engine: Config,
    /// Pixel-art banner / HUD style.
    pub text: BannerStyle,
    /// The glyph source the display-height banners (titles, verdicts, the
    /// masked word — the `banner_scale` family) and the reject cross render
    /// through — a 64px Menlo raster in the shipped config, resolved once at
    /// startup. The Rust `Default` is the neutral 8×8 bitmap; the product look
    /// comes from the bundled JSON.
    pub banner_glyphs: GlyphSourceConfig,
    /// The glyph source for body-height text (the HUD line, menu rows, board
    /// rows, readouts — the `hud_scale` family). `None` (the Rust default)
    /// shares `banner_glyphs`; the shipped config sets a smaller raster (32px
    /// Menlo) so both text sizes render at the full resolution their height
    /// allows.
    pub hud_glyphs: Option<GlyphSourceConfig>,
    /// Correct / wrong answer feedback colours and timing.
    pub feedback: FeedbackBeatConfig,
    /// The per-question timer bar's colours (its on-screen rect is an app
    /// layout value; the gauge is the reusable `ratgames::MeterBar`).
    pub timer_bar: MeterBarConfig,
    /// Level Intro / Level Clear screen hold timing — a reusable `ratgames`
    /// countdown config; the product value lives in the bundled JSON.
    pub interstitial: CountdownConfig,
    /// High-score board capacity and save file.
    pub scores: ScoresConfig,
    /// Run-wide starting lives. The per-level rules — clear/fail goal, reward,
    /// and clock — live in the level files, not here.
    pub starting_lives: u32,
    /// Points awarded per whole second left on the clock when a question is
    /// answered correctly (`0` = no time bonus). The per-level time limit
    /// itself is authored in the level files.
    pub time_bonus_per_second: u32,
    /// Run-wide arcade scoring: the combo bonus, perfect-clear bonus, and 1UP
    /// thresholds with a lives cap. A reusable `ratgames` rules type; the
    /// product values live in the bundled JSON.
    pub scoring: ScoringRules,
    /// Rank-based endings, proudest first — the result screen shows the first
    /// rank a finished run earns instead of the plain win / game-over title. A
    /// reusable `ratgames` rules type; the product titles live in the bundled
    /// JSON. Empty (the Rust default) keeps the plain titles.
    pub ranks: RankRules,
    /// The arcade continue policy: how many continues a run may use and
    /// whether the score survives one. A reusable `ratgames` rules type; the
    /// product values live in the bundled JSON. The Rust default offers none.
    pub continues: ContinueRules,
    /// How long the game-over CONTINUE? prompt holds before declining — a
    /// reusable `ratgames` countdown config; the product value lives in the
    /// bundled JSON.
    pub continue_prompt: CountdownConfig,
    /// Attract-mode timing: the title's idle trigger and the per-card hold.
    /// The Rust default leaves attract mode off; the shipped values turn it on.
    pub attract: AttractConfig,
    /// The selectable difficulties, in menu order. Empty (the Rust default)
    /// skips the select screen and plays the gauntlet exactly as authored,
    /// with the run-wide `starting_lives` above.
    pub difficulties: Vec<DifficultyPreset>,
    /// Every user-facing string. Blank by default; the product copy lives in
    /// the bundled `copy.json`, merged in at load.
    pub copy: CopyConfig,
    /// Where every screen element sits. Neutral by default; the product
    /// positions live in the bundled `layout.json`, merged in at load.
    pub layout: LayoutConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        // A playable neutral default: three lives, like an arcade run. The
        // named faces and product look still come from the bundled JSON, not
        // here.
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
            copy: CopyConfig::default(),
            layout: LayoutConfig::default(),
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

/// The bundled default, composed from per-domain files under `config/` and
/// parsed once. `engine.json` holds the ratgames engine config; `style.json`
/// the visual style (text scale/shadow, banner glyphs, feedback, timer bar);
/// `economy.json` the run economy and pacing (lives, scoring, ranks,
/// continues, attract, interstitials, difficulties); `copy.json` every
/// user-facing string (under the `copy` key); `layout.json` every on-screen
/// position (under the `layout` key). Every root key is authored in exactly
/// one file — a collision panics rather than letting file order decide. The
/// merged object deserialises into one [`AppConfig`]. A malformed bundle is
/// caught by the unit tests below (a build-time guarantee), not left as a
/// runtime risk.
static BUNDLED: LazyLock<AppConfig> = LazyLock::new(|| {
    let mut root = serde_json::Map::new();
    merge_domain(&mut root, "engine.json", include_str!("engine.json"));
    merge_domain(&mut root, "style.json", include_str!("style.json"));
    merge_domain(&mut root, "economy.json", include_str!("economy.json"));
    insert_domain_key(
        &mut root,
        "copy",
        bundled_json(include_str!("copy.json"), "copy.json"),
    );
    insert_domain_key(
        &mut root,
        "layout",
        bundled_json(include_str!("layout.json"), "layout.json"),
    );
    serde_json::from_value(serde_json::Value::Object(root))
        .expect("bundled config must deserialise into AppConfig")
});

/// Merge a root-spanning per-domain file (its top-level keys become root
/// config keys) into the bundle, panicking if a key was already supplied by an
/// earlier file — a build-time guarantee, like the parses themselves.
fn merge_domain(root: &mut serde_json::Map<String, serde_json::Value>, name: &str, text: &str) {
    let serde_json::Value::Object(map) = bundled_json(text, name) else {
        panic!("bundled config/{name} must be a JSON object");
    };
    for (key, value) in map {
        insert_domain_key(root, &key, value);
    }
}

/// Insert one root config key, panicking on a duplicate across domain files.
fn insert_domain_key(
    root: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: serde_json::Value,
) {
    assert!(
        root.insert(key.to_string(), value).is_none(),
        "bundled config key {key:?} is supplied by more than one per-domain file"
    );
}

/// Parse a bundled per-domain config file, panicking on a malformed bundle — a
/// build-time guarantee, since these are `include_str!`'d at compile time.
fn bundled_json(text: &str, name: &str) -> serde_json::Value {
    serde_json::from_str(text)
        .unwrap_or_else(|_| panic!("bundled config/{name} must be valid JSON"))
}

impl AppConfig {
    /// The config for this run: the `--config <path>` file if one was given,
    /// else the bundled default. Both are validated before use.
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

    /// The app's own invariants plus the engine's and the reusable widgets'.
    /// Each component checks itself (`Config::validate` covers the window /
    /// screen / input font; the ratgames widget configs cover their own
    /// scales, offsets, timings, and files); here we keep only the app
    /// composition — starting lives — and the checks that span components.
    fn validate(&self) -> Result<(), AppConfigError> {
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
        // lives-cap-vs-starting-lives cross-check needs the run and is
        // enforced when the session applies the rules (`GameRun::set_scoring`).
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
            // The same cross-check the session applies at startup: a preset
            // the scoring lives cap forbids would fail mid-flow at select
            // time, so catch it here where the whole config is in view.
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

/// The bundled default gauntlet, embedded at compile time and parsed once —
/// one `level_<n>.json` per level, in order. A malformed bundle is caught by
/// the unit test below, not left as a runtime risk.
static BUNDLED_LEVELS: LazyLock<Vec<WordLevel>> = LazyLock::new(|| {
    const FILES: &[&str] = &[
        include_str!("levels/level_0.json"),
        include_str!("levels/level_1.json"),
        include_str!("levels/level_2.json"),
        include_str!("levels/level_3.json"),
        include_str!("levels/level_4.json"),
        include_str!("levels/level_5.json"),
        include_str!("levels/level_6.json"),
        include_str!("levels/level_7.json"),
    ];
    FILES
        .iter()
        .map(|text| {
            serde_json::from_str(text).expect("bundled config/levels/level_<n>.json must be valid")
        })
        .collect()
});

/// The bundled word pool, embedded at compile time and validated once — a JSON
/// array of words in `words.json`. A malformed pool is caught by the unit test
/// below, not left as a runtime risk.
static BUNDLED_WORDS: LazyLock<WordList> = LazyLock::new(|| {
    let words: Vec<String> = serde_json::from_str(include_str!("words.json"))
        .expect("bundled config/words.json must be a JSON array of strings");
    WordList::new(words).expect("bundled config/words.json must be a valid word pool")
});

/// The word pool the gauntlet poses from.
#[must_use]
pub fn bundled_words() -> WordList {
    BUNDLED_WORDS.clone()
}

/// The levels for this run, in order: the `--levels <dir>` directory's
/// `level_<n>.json` files (sorted by index) if given, else the bundled
/// gauntlet.
///
/// Level *content* is validated later, when the session builds the campaign
/// from these (unbuildable word shapes, unplayable goals); this only reads and
/// parses.
///
/// # Errors
/// [`AppConfigError`] if the directory cannot be read, holds no
/// `level_<n>.json` files, or a file cannot be read or parsed.
pub fn resolve_levels(cli_dir: Option<PathBuf>) -> Result<Vec<WordLevel>, AppConfigError> {
    match cli_dir {
        Some(dir) => Ok(load_levels_dir::<Words>(&dir)?),
        None => Ok(BUNDLED_LEVELS.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratgames::{AnswerMode, FontFamily, FontSource, FontWeight};
    use wordgame_app::WordgameSession;
    use wordgame_core::Rng;

    #[test]
    fn bundled_default_selects_the_product_structure() {
        // The bundled JSON is the source of truth for the product look, not a
        // Rust literal. Pin only the structural choices — the faces (Menlo
        // Bold, input and banners), the screen geometry, the banner glyph
        // source and its resolution, where scores persist. The continuous
        // scalars (scales, offsets, sizes, thresholds, colours, frame counts)
        // are live-tuned in the per-domain files while the window runs;
        // exact-value pins broke the mathgame twin of this test on every
        // tuning pass, and `resolve()` already validates their invariants.
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
                assert_eq!(*cell_px, 64);
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
            other => panic!("expected a 64px raster banner source, got {other:?}"),
        }
        match config
            .hud_glyphs
            .as_ref()
            .expect("a hud glyph source is shipped")
        {
            GlyphSourceConfig::Raster { cell_px, font, .. } => {
                assert_eq!(*cell_px, 32);
                match font {
                    FontSource::System {
                        family: FontFamily::Named(name),
                        ..
                    } => assert_eq!(name, "Menlo"),
                    other => panic!("expected a Menlo raster font, got {other:?}"),
                }
            }
            other => panic!("expected a 32px raster hud source, got {other:?}"),
        }
        // The neutral Default shares the banner source (no second source).
        assert!(AppConfig::default().hud_glyphs.is_none());
        assert_eq!(config.scores.capacity, 10);
        assert_eq!(
            config.scores.file,
            std::path::PathBuf::from("wordgame-highscores.json")
        );
        // Run-wide starting lives are shipped game *design* — pin them. The
        // per-level rules live in the level files, pinned by the ladder test.
        assert_eq!(config.starting_lives, 3);
    }

    #[test]
    fn bundled_domain_files_hold_disjoint_keys() {
        // Every root config key is authored in exactly one per-domain file; a
        // duplicate would make the bundle order-dependent. The loader already
        // panics on a collision — this pins the authored bundle as clean with
        // a readable failure instead of a poisoned LazyLock.
        let domains = [
            ("engine.json", include_str!("engine.json")),
            ("style.json", include_str!("style.json")),
            ("economy.json", include_str!("economy.json")),
        ];
        let mut seen: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
        for (name, text) in domains {
            let value: serde_json::Value = serde_json::from_str(text).expect(name);
            let object = value.as_object().expect("domain file must be an object");
            for key in object.keys() {
                assert!(
                    !matches!(key.as_str(), "copy" | "layout"),
                    "{name} must not supply the whole-file domain key {key:?}"
                );
                if let Some(previous) = seen.insert(key.clone(), name) {
                    panic!("config key {key:?} appears in both {previous} and {name}");
                }
            }
        }
    }

    #[test]
    fn bundled_ranks_name_the_shipped_endings() {
        // The rank table is shipped game design: pin its shape — the two win
        // endings, proudest first, every rule win-gated so a game over keeps
        // its plain title. The failure/point thresholds stay tunable.
        let config = AppConfig::resolve(None).expect("bundled config");
        let titles: Vec<_> = config
            .ranks
            .rules
            .iter()
            .map(|rule| rule.title.as_str())
            .collect();
        assert_eq!(titles, vec!["NO MISS CHAMP", "WORD WIZARD"]);
        assert!(
            config.ranks.rules.iter().all(|rule| rule.requires_won),
            "a lost run keeps the plain GAME OVER title"
        );
    }

    #[test]
    fn bundled_continues_offer_one_score_keeping_continue() {
        let config = AppConfig::resolve(None).expect("bundled config");
        assert!(config.continues.allowed >= 1);
        assert!(config.continue_prompt.frames >= 1);
    }

    #[test]
    fn bundled_attract_mode_is_on_with_a_real_rotation() {
        let config = AppConfig::resolve(None).expect("bundled config");
        assert!(config.attract.idle.frames > 0, "attract mode is on");
        assert!(config.attract.card.frames > 0);
        assert!(config.attract.idle_countdown().is_some());
    }

    #[test]
    fn bundled_words_form_a_valid_pool_with_ladder_coverage() {
        // The pool must build (validated words, no duplicates) and offer a
        // real choice at every ladder shape — a level with only a handful of
        // eligible words would repeat itself within one clear.
        let words = bundled_words();
        for level in resolve_levels(None).expect("bundled levels") {
            let generator = level
                .content
                .generator(&words)
                .unwrap_or_else(|e| panic!("level {:?} must build: {e}", level.name));
            assert!(
                generator.word_count() >= 20,
                "level {:?} draws from only {} words",
                level.name,
                generator.word_count()
            );
        }
    }

    #[test]
    fn bundled_levels_form_the_graduated_ladder() {
        // Structure, not values: the ladder starts at short words with one
        // blank, grows monotonically in both word length and hidden letters,
        // keeps every level typed (the input field, one letter at a time) and
        // timed with a non-increasing clock, and builds a playable session
        // over the bundled pool. The exact ranges, points, and clocks stay
        // tunable data.
        let levels = resolve_levels(None).expect("bundled levels");
        assert_eq!(levels.len(), 8);

        let first = &levels[0];
        assert_eq!(
            (first.content.length_min, first.content.length_max),
            (3, 4),
            "the ladder opens on three-or-four-letter words"
        );
        assert_eq!(first.content.blanks, 1, "the ladder opens on one blank");

        let last = levels.last().expect("eight levels");
        assert!(
            last.content.length_min >= 7,
            "the summit poses the longest words"
        );
        assert!(last.content.blanks >= 3, "the summit hides several letters");

        for pair in levels.windows(2) {
            assert!(
                pair[1].content.length_min >= pair[0].content.length_min
                    && pair[1].content.length_max >= pair[0].content.length_max,
                "word lengths never shrink: {:?} -> {:?}",
                pair[0].name,
                pair[1].name
            );
            assert!(
                pair[1].content.blanks >= pair[0].content.blanks,
                "blanks never shrink: {:?} -> {:?}",
                pair[0].name,
                pair[1].name
            );
            assert!(
                pair[1].rules.time_limit_frames <= pair[0].rules.time_limit_frames,
                "the clock never loosens: {:?} -> {:?}",
                pair[0].name,
                pair[1].name
            );
        }

        for level in &levels {
            assert!(!level.name.is_empty());
            assert!(!level.difficulty.is_empty());
            assert_eq!(
                level.rules.answer_mode,
                AnswerMode::Typed,
                "level {:?} answers with the input field",
                level.name
            );
            assert!(level.rules.time_limit_frames > 0, "every level is timed");
            assert!(level.rules.required_successes > 0);
            assert!(level.rules.points_per_success > 0);
        }

        // The whole ladder builds a playable session over the bundled pool.
        assert!(
            WordgameSession::from_levels(&levels, &bundled_words(), 3, 1).is_ok(),
            "the bundled gauntlet must build a session"
        );
    }

    #[test]
    fn every_bundled_level_generates_puzzles_of_its_own_shape() {
        // Session-build alone only generates from level 0 — drive every
        // level's generator directly so a data mistake in any file surfaces.
        let words = bundled_words();
        for level in resolve_levels(None).expect("bundled levels") {
            let generator = level.content.generator(&words).expect("buildable level");
            let mut rng = Rng::new(0x5745_4c4c);
            for _ in 0..100 {
                let puzzle = generator.generate(&mut rng);
                let length = puzzle.word().chars().count();
                assert!(
                    (level.content.length_min..=level.content.length_max).contains(&length),
                    "level {:?} posed {:?}",
                    level.name,
                    puzzle
                );
                assert_eq!(puzzle.blank_count(), level.content.blanks);
                assert!(puzzle.masked().contains('_'));
            }
        }
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
}

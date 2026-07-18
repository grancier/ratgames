//! The app's configuration: `ratgames::Config` (the engine) plus this app's
//! maze shape, scene geometry, colours, and copy — sourced from data, not
//! hardcoded in Rust.
//!
//! The default lives in a bundled `defaults.json`, embedded at compile time
//! and parsed once, so `cargo run -p mazegame-app` needs no external file yet
//! no product value — the maze size, the tile size, the colours, the HUD copy
//! — is baked into a Rust literal. A `--config <path>` flag overrides it with
//! a single TOML or JSON file (read through `ratgames::load_config_file`).
//! Rust holds only the config *types* and their neutral `Default` fallbacks.

use std::path::PathBuf;
use std::sync::LazyLock;

use ratgames::{
    BannerStyle, Color, Config, ConfigError, ConfigFileError, GlyphSourceConfig, Point,
    load_config_file, palette,
};

/// One rung of the ladder: the maze this level deals and how it draws. Easy
/// levels use few cells at a large tile (short, wide corridors, few
/// deviations); the ladder narrows and branches as it climbs. (A maze of
/// `w × h` cells renders as `(2w+1) × (2h+1)` tiles.) Every field is explicit
/// in the level data — a rung is authored whole, not defaulted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct MazeLevel {
    /// Maze width in cells.
    pub cells_w: usize,
    /// Maze height in cells.
    pub cells_h: usize,
    /// Tile edge in virtual pixels — the bar thickness, corridor width,
    /// block size, and step distance in one knob.
    pub tile_px: u32,
    /// Digits scattered over the floor, valued `1..=n`, gathered in order
    /// before the exit opens.
    pub digits: usize,
    /// Growing-tree branching 0–100: 0 carves long winding corridors with few
    /// deviations; higher values branch into many short spurs.
    pub branch_chance: u8,
}

impl MazeLevel {
    /// The tile-grid width and height this level's maze renders as.
    #[must_use]
    pub fn grid_tiles(&self) -> (usize, usize) {
        (2 * self.cells_w + 1, 2 * self.cells_h + 1)
    }
}

/// The scene's colours, `#RRGGBB` / `#AARRGGBB` strings in config.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct SceneColors {
    /// The solid bars.
    pub wall: Color,
    /// The player's block.
    pub player: Color,
    /// Digits whose turn has not come yet (solid, like the bars).
    pub digit: Color,
    /// The digit due next — the only one that can be collected.
    pub digit_next: Color,
    /// The exit tile while numbers remain.
    pub exit_locked: Color,
    /// The exit tile once every number is collected.
    pub exit_open: Color,
}

impl Default for SceneColors {
    fn default() -> Self {
        // Neutral palette fallbacks; the product colours live in the bundled
        // JSON.
        Self {
            wall: palette::OUTLINE,
            player: palette::ACCENT,
            digit: palette::INK,
            digit_next: palette::FILL,
            exit_locked: palette::DANGER,
            exit_open: palette::WARNING,
        }
    }
}

/// How the maze draws on the virtual screen. Each level brings its own tile
/// size, so the maze itself is *centred* in the space below the HUD strip
/// rather than anchored at an authored origin.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct SceneConfig {
    /// Virtual pixels reserved at the top for the HUD line; the maze centres
    /// in the remaining band.
    pub hud_strip: u32,
    /// Top-left anchor of the HUD line (inside the strip).
    pub hud_at: Point,
    pub colors: SceneColors,
}

impl Default for SceneConfig {
    fn default() -> Self {
        // Neutral: no strip, HUD at the origin — the product geometry lives
        // only in the bundled JSON.
        Self {
            hud_strip: 0,
            hud_at: Point::ORIGIN,
            colors: SceneColors::default(),
        }
    }
}

/// Every user-facing string. Blank by default; the product copy lives in the
/// bundled JSON.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(default)]
pub struct CopyConfig {
    /// HUD line — three `{}` (level number, collected, total).
    pub hud: String,
    /// Level-clear banner — one `{}` (the level number just cleared).
    pub level_clear: String,
    /// The run-won banner, after the last level.
    pub win: String,
}

/// The whole app config: the reusable engine config plus this app's level
/// ladder, scene geometry, and copy.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Window, screen, and theme.
    pub engine: Config,
    /// Banner / HUD magnification and drop shadow (a reusable `ratgames`
    /// style; the win banner uses `banner_scale`, the HUD line `hud_scale`).
    pub text: BannerStyle,
    /// The glyph source the HUD line and the win banner render through — a
    /// 32px Menlo raster in the shipped config, resolved once at startup. The
    /// Rust `Default` is the neutral 8×8 bitmap; the product look comes from
    /// the bundled JSON. (The in-maze digits are not text: they are game
    /// pieces sized to their tiles, baked from the 8×8 bitmap regardless.)
    pub glyphs: GlyphSourceConfig,
    /// The ladder, easiest first. Clearing a level advances to the next;
    /// clearing the last wins the run.
    pub levels: Vec<MazeLevel>,
    pub scene: SceneConfig,
    pub copy: CopyConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        // A single playable neutral rung; the shipped ladder lives in the
        // bundled JSON.
        Self {
            engine: Config::default(),
            text: BannerStyle::default(),
            glyphs: GlyphSourceConfig::default(),
            levels: vec![MazeLevel {
                cells_w: 2,
                cells_h: 2,
                tile_px: 1,
                digits: 1,
                branch_chance: 0,
            }],
            scene: SceneConfig::default(),
            copy: CopyConfig::default(),
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
}

/// The bundled default, embedded at compile time and parsed once. A malformed
/// bundle is caught by the unit tests below (a build-time guarantee), not left
/// as a runtime risk.
static BUNDLED: LazyLock<AppConfig> = LazyLock::new(|| {
    serde_json::from_str(include_str!("defaults.json"))
        .expect("bundled config/defaults.json must deserialise into AppConfig")
});

impl AppConfig {
    /// The config for this run: the `--config <path>` file if one was given,
    /// else the bundled default. Both are validated before use.
    ///
    /// # Errors
    /// [`AppConfigError`] if a file source cannot be read, parsed, or fails
    /// validation.
    pub fn resolve(cli_path: Option<PathBuf>) -> Result<Self, AppConfigError> {
        let config = match cli_path {
            Some(path) => load_config_file(&path)?,
            None => BUNDLED.clone(),
        };
        config.validate()?;
        Ok(config)
    }

    /// The app's own invariants plus the engine's and the text style's. Each
    /// component checks itself; here we keep the app composition — a ladder
    /// that exists and whose every rung deals single-digit numbers a perfect
    /// maze can hold, drawn inside the virtual screen below the HUD strip.
    fn validate(&self) -> Result<(), AppConfigError> {
        self.text
            .validate()
            .map_err(|e| AppConfigError::Invalid(format!("text: {e}")))?;
        if self.levels.is_empty() {
            return Err(AppConfigError::Invalid(
                "levels must hold at least one rung".to_string(),
            ));
        }
        let screen = self.engine.screen.size;
        for (index, level) in self.levels.iter().enumerate() {
            if level.cells_w == 0 || level.cells_h == 0 {
                return Err(AppConfigError::Invalid(format!(
                    "levels[{index}]: cells_w / cells_h must be at least 1"
                )));
            }
            if !(1..=9).contains(&level.digits) {
                return Err(AppConfigError::Invalid(format!(
                    "levels[{index}]: digits must be 1..=9 (each shows a single digit)"
                )));
            }
            // A perfect maze of c cells has 2c-1 floor tiles; minus the start
            // and exit, that is the floor the digits can land on.
            let free_floor = 2 * level.cells_w * level.cells_h - 3;
            if level.digits > free_floor {
                return Err(AppConfigError::Invalid(format!(
                    "levels[{index}]: {} digits need more free floor than a \
                     {}x{}-cell maze has ({free_floor})",
                    level.digits, level.cells_w, level.cells_h
                )));
            }
            if level.tile_px == 0 {
                return Err(AppConfigError::Invalid(format!(
                    "levels[{index}]: tile_px must be at least 1"
                )));
            }
            if level.branch_chance > 100 {
                return Err(AppConfigError::Invalid(format!(
                    "levels[{index}]: branch_chance is a percentage (0..=100)"
                )));
            }
            // The whole tile grid must land on the virtual screen below the
            // HUD strip — a maze poking past the edge reads as a mistyped
            // size, not a design.
            let (grid_w, grid_h) = level.grid_tiles();
            let needed_w = grid_w as u64 * u64::from(level.tile_px);
            let needed_h =
                u64::from(self.scene.hud_strip) + grid_h as u64 * u64::from(level.tile_px);
            if needed_w > u64::from(screen.w) || needed_h > u64::from(screen.h) {
                return Err(AppConfigError::Invalid(format!(
                    "levels[{index}] needs {needed_w}x{needed_h} virtual pixels \
                     but the screen is {}x{}",
                    screen.w, screen.h
                )));
            }
        }
        self.engine.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratgames::{FontFamily, FontSource, FontWeight, Size};

    #[test]
    fn bundled_default_selects_the_product_structure() {
        // The bundled JSON is the source of truth for the product shape — pin
        // the structural choices (the HUD band, the shipped copy, the 32px
        // raster text source); the tunables (colours, wording, exact sizes)
        // stay freely editable data. The ladder's own shape is pinned by
        // `the_bundled_ladder_climbs_as_specified`.
        let config = AppConfig::resolve(None).expect("bundled config must be valid");
        assert_eq!(config.engine.window.title, "MAZE GAME");
        assert!(config.scene.hud_strip > 0, "the HUD has its own band");
        assert!(!config.copy.hud.is_empty());
        assert!(!config.copy.level_clear.is_empty());
        assert!(!config.copy.win.is_empty());
        // The HUD and win banner render through a 32px raster, like the
        // sibling games' body text — never the chunky 8×8 bitmap.
        match &config.glyphs {
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
                    other => panic!("expected a named system font, got {other:?}"),
                }
            }
            other => panic!("expected a 32px raster text source, got {other:?}"),
        }
        // The neutral Rust default stays the bitmap (no font baked into Rust).
        assert!(matches!(
            AppConfig::default().glyphs,
            GlyphSourceConfig::Bitmap8x8
        ));
    }

    #[test]
    fn the_neutral_rust_default_validates() {
        assert!(AppConfig::default().validate().is_ok());
    }

    #[test]
    #[ignore = "resolves the Menlo system font; run via cargo test -- --ignored"]
    fn the_bundled_glyph_source_resolves() {
        let config = AppConfig::resolve(None).expect("bundled config");
        assert!(
            config.glyphs.resolve().is_ok(),
            "the shipped 32px raster source must load its font"
        );
    }

    #[test]
    fn validate_rejects_degenerate_values() {
        let base = AppConfig::default;
        let mut no_levels = base();
        no_levels.levels.clear();
        assert!(matches!(
            no_levels.validate(),
            Err(AppConfigError::Invalid(_))
        ));

        let mut zero_cells = base();
        zero_cells.levels[0].cells_w = 0;
        assert!(matches!(
            zero_cells.validate(),
            Err(AppConfigError::Invalid(_))
        ));

        let mut two_digits = base();
        two_digits.levels[0].digits = 10;
        assert!(matches!(
            two_digits.validate(),
            Err(AppConfigError::Invalid(_))
        ));

        let mut crowded = base();
        crowded.levels[0] = MazeLevel {
            cells_w: 2,
            cells_h: 1,
            tile_px: 1,
            digits: 2, // a 2x1-cell maze frees exactly one floor tile
            branch_chance: 0,
        };
        assert!(matches!(
            crowded.validate(),
            Err(AppConfigError::Invalid(_))
        ));

        let mut no_tiles = base();
        no_tiles.levels[0].tile_px = 0;
        assert!(matches!(
            no_tiles.validate(),
            Err(AppConfigError::Invalid(_))
        ));

        let mut over_percent = base();
        over_percent.levels[0].branch_chance = 101;
        assert!(matches!(
            over_percent.validate(),
            Err(AppConfigError::Invalid(_))
        ));
    }

    #[test]
    fn validate_rejects_a_level_that_pokes_past_the_screen() {
        let mut config = AppConfig::default();
        config.engine.screen.size = Size::new(320, 180);
        config.scene.hud_strip = 20;
        config.levels[0] = MazeLevel {
            cells_w: 16, // 33 tiles * 10px = 330 > 320
            cells_h: 7,
            tile_px: 10,
            digits: 3,
            branch_chance: 0,
        };
        assert!(matches!(config.validate(), Err(AppConfigError::Invalid(_))));

        config.levels[0].cells_w = 15; // 31 tiles * 10px = 310 <= 320
        assert!(config.validate().is_ok());

        // The HUD strip counts against the height.
        config.levels[0].cells_h = 7; // 15 tiles * 10px = 150; 150 + 40 > 180
        config.scene.hud_strip = 40;
        assert!(matches!(config.validate(), Err(AppConfigError::Invalid(_))));
    }

    #[test]
    fn the_bundled_ladder_climbs_as_specified() {
        // Structure, not values: seven rungs; the digits count 3,4,...,9 (one
        // more per level); corridors start wide and never widen again; the
        // branching never relaxes; every rung fits the screen (resolve()
        // validates) and actually deals a playable maze. The exact cell
        // counts, tile sizes, and colours stay freely tunable data.
        let config = AppConfig::resolve(None).expect("bundled config");
        let levels = &config.levels;
        assert_eq!(levels.len(), 7);
        for (index, level) in levels.iter().enumerate() {
            assert_eq!(level.digits, index + 3, "digits climb 3..=9");
            assert!(
                mazegame_core::MazeGame::new(
                    level.cells_w,
                    level.cells_h,
                    level.digits,
                    level.branch_chance,
                    1
                )
                .is_ok(),
                "level {index} must deal"
            );
        }
        for pair in levels.windows(2) {
            assert!(
                pair[1].tile_px <= pair[0].tile_px,
                "corridors never widen as the ladder climbs"
            );
            assert!(
                pair[1].branch_chance >= pair[0].branch_chance,
                "branching never relaxes as the ladder climbs"
            );
        }
        assert!(
            levels.first().expect("seven rungs").tile_px
                > levels.last().expect("seven rungs").tile_px,
            "the ladder narrows overall"
        );
    }
}

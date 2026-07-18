//! The app's configuration: `ratgames::Config` (the engine) plus this app's
//! maze shape, scene geometry, colours, and copy — sourced from data, not
//! hardcoded in Rust.
//!
//! The default lives in a bundled `defaults.json`, embedded at compile time
//! and parsed once, so `cargo run -p mazegame-app` needs no external file yet
//! no product value — the maze size, the 10px tile, the colours, the HUD copy
//! — is baked into a Rust literal. A `--config <path>` flag overrides it with
//! a single TOML or JSON file (read through `ratgames::load_config_file`).
//! Rust holds only the config *types* and their neutral `Default` fallbacks.

use std::path::PathBuf;
use std::sync::LazyLock;

use ratgames::{
    BannerStyle, Color, Config, ConfigError, ConfigFileError, Point, load_config_file, palette,
};

/// The maze the run poses: its size in cells and how many numbers it hides.
/// (A maze of `w × h` cells renders as `(2w+1) × (2h+1)` tiles.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(default)]
pub struct MazeConfig {
    /// Maze width in cells.
    pub cells_w: usize,
    /// Maze height in cells.
    pub cells_h: usize,
    /// Numbers scattered over the floor, valued `1..=n` — all must be
    /// collected before the exit opens.
    pub collectibles: usize,
}

impl Default for MazeConfig {
    fn default() -> Self {
        // A playable neutral default; the shipped shape lives in the bundled
        // JSON.
        Self {
            cells_w: 2,
            cells_h: 2,
            collectibles: 1,
        }
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
    /// The digits waiting on the floor.
    pub digit: Color,
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
            digit: palette::FILL,
            exit_locked: palette::DANGER,
            exit_open: palette::WARNING,
        }
    }
}

/// Where and how large the maze draws on the virtual screen, in virtual
/// pixels.
#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct SceneConfig {
    /// Tile edge in virtual pixels — the bar width and the block size (the
    /// shipped config says 10, so the bars are 10px and one step is one 10px
    /// move).
    pub tile_px: u32,
    /// Top-left corner of the maze on the virtual screen.
    pub origin: Point,
    /// Top-left anchor of the collected-count HUD line.
    pub hud_at: Point,
    pub colors: SceneColors,
}

impl Default for SceneConfig {
    fn default() -> Self {
        // Neutral: identity tiles at the origin, so the product geometry lives
        // only in the bundled JSON.
        Self {
            tile_px: 1,
            origin: Point::ORIGIN,
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
    /// Collected-count HUD line — two `{}` (collected, total).
    pub hud: String,
    /// The win banner.
    pub win: String,
}

/// The whole app config: the reusable engine config plus this app's maze
/// shape, scene geometry, and copy.
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(default)]
pub struct AppConfig {
    /// Window, screen, and theme.
    pub engine: Config,
    /// Banner / HUD magnification and drop shadow (a reusable `ratgames`
    /// style; the win banner uses `banner_scale`, the HUD line `hud_scale`).
    pub text: BannerStyle,
    pub maze: MazeConfig,
    pub scene: SceneConfig,
    pub copy: CopyConfig,
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

    /// The maze's tile-grid width and height for this config's cell counts.
    #[must_use]
    pub fn grid_tiles(&self) -> (usize, usize) {
        (2 * self.maze.cells_w + 1, 2 * self.maze.cells_h + 1)
    }

    /// The app's own invariants plus the engine's and the text style's. Each
    /// component checks itself; here we keep the app composition — a maze that
    /// exists, single-digit numbers, and a scene that fits the virtual screen.
    fn validate(&self) -> Result<(), AppConfigError> {
        self.text
            .validate()
            .map_err(|e| AppConfigError::Invalid(format!("text: {e}")))?;
        if self.maze.cells_w == 0 || self.maze.cells_h == 0 {
            return Err(AppConfigError::Invalid(
                "maze.cells_w / cells_h must be at least 1".to_string(),
            ));
        }
        if !(1..=9).contains(&self.maze.collectibles) {
            return Err(AppConfigError::Invalid(
                "maze.collectibles must be 1..=9 (each shows a single digit)".to_string(),
            ));
        }
        if self.scene.tile_px == 0 {
            return Err(AppConfigError::Invalid(
                "scene.tile_px must be at least 1".to_string(),
            ));
        }
        if self.scene.origin.x < 0 || self.scene.origin.y < 0 {
            return Err(AppConfigError::Invalid(
                "scene.origin must not be negative".to_string(),
            ));
        }
        // The whole tile grid must land on the virtual screen — a maze poking
        // past the edge reads as a mistyped size, not a design.
        let (grid_w, grid_h) = self.grid_tiles();
        let needed_w = self.scene.origin.x as i64 + grid_w as i64 * i64::from(self.scene.tile_px);
        let needed_h = self.scene.origin.y as i64 + grid_h as i64 * i64::from(self.scene.tile_px);
        let screen = self.engine.screen.size;
        if needed_w > i64::from(screen.w) || needed_h > i64::from(screen.h) {
            return Err(AppConfigError::Invalid(format!(
                "the maze needs {needed_w}x{needed_h} virtual pixels but the screen is {}x{}",
                screen.w, screen.h
            )));
        }
        self.engine.validate()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratgames::Size;

    #[test]
    fn bundled_default_selects_the_product_structure() {
        // The bundled JSON is the source of truth for the product shape — pin
        // the structural choices (10px tiles, a maze that fills the screen,
        // single-digit numbers); the tunables (colours, copy wording, exact
        // cell counts) stay freely editable data.
        let config = AppConfig::resolve(None).expect("bundled config must be valid");
        assert_eq!(config.scene.tile_px, 10, "the POC brief: 10px bars");
        assert!(config.maze.cells_w >= 8 && config.maze.cells_h >= 4);
        assert!((1..=9).contains(&config.maze.collectibles));
        assert_eq!(config.engine.window.title, "MAZE GAME");
        assert!(!config.copy.hud.is_empty());
        assert!(!config.copy.win.is_empty());
    }

    #[test]
    fn the_neutral_rust_default_validates() {
        assert!(AppConfig::default().validate().is_ok());
    }

    #[test]
    fn validate_rejects_degenerate_values() {
        let base = AppConfig::default;
        let mut zero_cells = base();
        zero_cells.maze.cells_w = 0;
        assert!(matches!(
            zero_cells.validate(),
            Err(AppConfigError::Invalid(_))
        ));

        let mut two_digits = base();
        two_digits.maze.collectibles = 10;
        assert!(matches!(
            two_digits.validate(),
            Err(AppConfigError::Invalid(_))
        ));

        let mut no_tiles = base();
        no_tiles.scene.tile_px = 0;
        assert!(matches!(
            no_tiles.validate(),
            Err(AppConfigError::Invalid(_))
        ));
    }

    #[test]
    fn validate_rejects_a_maze_that_pokes_past_the_screen() {
        let mut config = AppConfig::default();
        config.engine.screen.size = Size::new(320, 180);
        config.maze.cells_w = 16; // 33 tiles * 10px = 330 > 320
        config.maze.cells_h = 7;
        config.scene.tile_px = 10;
        assert!(matches!(config.validate(), Err(AppConfigError::Invalid(_))));

        config.maze.cells_w = 15; // 31 tiles * 10px = 310 <= 320
        assert!(config.validate().is_ok());
    }

    #[test]
    fn the_bundled_maze_fits_the_bundled_screen() {
        // resolve() validates, so this pins that the shipped geometry stays
        // self-consistent as either side is tuned.
        let config = AppConfig::resolve(None).expect("bundled config");
        let (grid_w, grid_h) = config.grid_tiles();
        assert!(grid_w as u32 * config.scene.tile_px <= config.engine.screen.size.w);
        assert!(grid_h as u32 * config.scene.tile_px <= config.engine.screen.size.h);
    }
}

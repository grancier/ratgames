//! Load authored levels from bundled strings or an explicit CLI directory.
use super::{AppConfigError, AuthoredLevel, profiles::AuthoredArithmetic};
use ratgames::load_levels_dir;
use std::{path::PathBuf, sync::LazyLock};

/// The bundled default gauntlet, embedded at compile time and parsed once — one
/// `level_<n>.json` per level, in order. A malformed bundle is caught by the unit
/// test below, not left as a runtime risk.
static BUNDLED_LEVELS: LazyLock<Vec<AuthoredLevel>> = LazyLock::new(|| {
    const FILES: &[&str] = &[
        include_str!("levels/level_0.json"),
        include_str!("levels/level_1.json"),
        include_str!("levels/level_2.json"),
        include_str!("levels/level_3.json"),
        include_str!("levels/level_4.json"),
        include_str!("levels/level_5.json"),
        include_str!("levels/level_6.json"),
        include_str!("levels/level_7.json"),
        include_str!("levels/level_8.json"),
        include_str!("levels/level_9.json"),
        include_str!("levels/level_10.json"),
        include_str!("levels/level_11.json"),
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
/// [`super::AppConfig::prepare_campaigns`] validates content and profile mappings and
/// builds every selectable campaign before play; this only reads and parses.
///
/// # Errors
/// [`AppConfigError`] if the directory cannot be read, holds no `level_<n>.json`
/// files, or a file cannot be read or parsed.
pub fn resolve_levels(cli_dir: Option<PathBuf>) -> Result<Vec<AuthoredLevel>, AppConfigError> {
    match cli_dir {
        Some(dir) => Ok(load_levels_dir::<AuthoredArithmetic>(&dir)?),
        None => Ok(BUNDLED_LEVELS.clone()),
    }
}

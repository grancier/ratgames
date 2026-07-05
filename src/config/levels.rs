//! Level authoring and loading: a generic per-level config shape and the
//! directory loader that discovers a gauntlet of `level_<n>.json` files.
//!
//! A game authors each level as one flat JSON file — the reusable per-level
//! rules ([`LevelSpec`]) plus that game's own challenge content — and
//! [`load_levels_dir`] reads a directory of them, ordered by index. This is
//! reusable machinery; the content type and the files themselves belong to the
//! game (`ratgames` never depends on a game's domain, so the challenge content
//! is a generic type parameter, never a math type).

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::session::LevelSpec;

/// One level of a gauntlet, as authored in a `level_<n>.json` file: a display
/// name and difficulty label, the reusable per-level [`LevelSpec`] rules (win
/// condition, reward, input mode), and a game-supplied `Content` describing the
/// challenge itself.
///
/// The rules and the content are both `#[serde(flatten)]`ed, so the file stays
/// one flat object — e.g. `{"name":"NUMBER YARD","difficulty":"EASY","operator":
/// "add","min":0,"max":9,"required_successes":5,...}`, where `operator`/`min`/
/// `max` come from a game's `Content`. This is the generic shell: a game defines
/// its own `Content` (the math-free seam — the toolkit never depends on a game's
/// domain), and the reusable *type* lives here while the product *values* live in
/// the game's level files.
///
/// Omitted rule fields fall back to [`LevelSpec`] defaults; `name`, `difficulty`,
/// and the content's own fields are required.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LevelConfig<Content> {
    /// The level's display name (e.g. `"NUMBER YARD"`).
    pub name: String,
    /// A difficulty label shown to the player (e.g. `"EASY"`).
    pub difficulty: String,
    /// The reusable per-level rules: win condition, reward, and input mode.
    #[serde(flatten)]
    pub rules: LevelSpec,
    /// The game-specific challenge content (e.g. an arithmetic operator + range).
    #[serde(flatten)]
    pub content: Content,
}

/// Why loading a directory of `level_<n>.json` files failed.
#[derive(Debug, thiserror::Error)]
pub enum LevelLoadError {
    /// A directory or file could not be read.
    #[error("failed to read levels from {path:?}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// A level file was not valid JSON for its level type.
    #[error("failed to parse level file {path:?}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// The directory held no `level_<n>.json` files.
    #[error("no level_<n>.json files found in {dir:?}")]
    NoLevels { dir: PathBuf },
    /// `--levels` was given without a directory argument.
    #[error("--levels requires a directory argument")]
    MissingDir,
}

/// Load every `level_<n>.json` in `dir` into a [`LevelConfig<Content>`], ordered
/// by `<n>`. Files whose names are not `level_<n>.json` are ignored, so a pack
/// directory can carry a README or notes alongside its levels.
///
/// The returned levels are only read and parsed; their *content* is validated
/// later, when a game builds a [`Campaign`](crate::session::Campaign) from them
/// (unplayable goals, and any domain rule the content carries).
///
/// # Errors
/// [`LevelLoadError`] if the directory cannot be read, holds no `level_<n>.json`
/// files, or a file cannot be read or parsed.
pub fn load_levels_dir<Content>(dir: &Path) -> Result<Vec<LevelConfig<Content>>, LevelLoadError>
where
    Content: DeserializeOwned,
{
    let entries = std::fs::read_dir(dir).map_err(|source| LevelLoadError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    let mut indexed: Vec<(usize, PathBuf)> = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|source| LevelLoadError::Io {
                path: dir.to_path_buf(),
                source,
            })?
            .path();
        if let Some(index) = level_index(&path) {
            indexed.push((index, path));
        }
    }
    if indexed.is_empty() {
        return Err(LevelLoadError::NoLevels {
            dir: dir.to_path_buf(),
        });
    }
    indexed.sort_by_key(|(index, _)| *index);
    indexed
        .iter()
        .map(|(_, path)| load_level_file(path))
        .collect()
}

/// The `<n>` in a `level_<n>.json` file name, or `None` if `path` is not one.
fn level_index(path: &Path) -> Option<usize> {
    if path.extension().and_then(|e| e.to_str()) != Some("json") {
        return None;
    }
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| stem.strip_prefix("level_"))
        .and_then(|index| index.parse().ok())
}

/// Read and parse one level file into a [`LevelConfig<Content>`].
fn load_level_file<Content>(path: &Path) -> Result<LevelConfig<Content>, LevelLoadError>
where
    Content: DeserializeOwned,
{
    let text = std::fs::read_to_string(path).map_err(|source| LevelLoadError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| LevelLoadError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

/// Extract `--levels <dir>` (or `--levels=<dir>`) from `args`, returning the
/// directory if present and the remaining arguments in order — so a game can pull
/// its level-pack flag before handing the rest to
/// [`parse_config_flag`](super::parse_config_flag) for `--config`.
///
/// # Errors
/// [`LevelLoadError::MissingDir`] if `--levels` appears without a directory.
pub fn take_levels_flag<I>(args: I) -> Result<(Option<PathBuf>, Vec<String>), LevelLoadError>
where
    I: IntoIterator<Item = String>,
{
    let mut levels: Option<PathBuf> = None;
    let mut rest: Vec<String> = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if let Some(dir) = arg.strip_prefix("--levels=") {
            levels = Some(PathBuf::from(dir));
        } else if arg == "--levels" {
            let dir = args.next().ok_or(LevelLoadError::MissingDir)?;
            levels = Some(PathBuf::from(dir));
        } else {
            rest.push(arg);
        }
    }
    Ok((levels, rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::AnswerMode;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A stand-in for a game's level content — deliberately unlike arithmetic, to
    /// prove the shell is generic and math-free.
    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct TestContent {
        shape: String,
        size: u32,
    }

    /// A fresh, unique temp directory path (not created). Uses the pid plus a
    /// process-local counter so parallel tests never collide — no wall clock,
    /// which the workflow sandbox forbids anyway.
    fn unique_temp_dir(prefix: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("{prefix}_{}_{n}", std::process::id()))
    }

    fn write_level(dir: &Path, index: usize, name: &str) {
        let json = format!(
            r#"{{"name":"{name}","difficulty":"EASY","shape":"square","size":1,"required_successes":5}}"#
        );
        std::fs::write(dir.join(format!("level_{index}.json")), json).expect("write level file");
    }

    #[test]
    fn parses_a_flat_file_with_flattened_rules_and_content() {
        // One flat object: name/difficulty are the shell's own fields, the rest
        // splits across the two flattened halves — LevelSpec (rules) and the
        // game's content — with disjoint keys.
        let level: LevelConfig<TestContent> = serde_json::from_str(
            r#"{"name":"ARENA","difficulty":"EASY","shape":"hex","size":6,"required_successes":7}"#,
        )
        .expect("valid level file");
        assert_eq!(level.name, "ARENA");
        assert_eq!(level.difficulty, "EASY");
        assert_eq!(
            level.content,
            TestContent {
                shape: "hex".to_string(),
                size: 6
            }
        );
        assert_eq!(level.rules.required_successes, 7);
        // Omitted rule fields fall back to LevelSpec defaults.
        assert_eq!(level.rules.max_failures, LevelSpec::default().max_failures);
        assert_eq!(level.rules.answer_mode, AnswerMode::Typed);
    }

    #[test]
    fn round_trips_through_json() {
        let level = LevelConfig {
            name: "ARENA".to_string(),
            difficulty: "HARD".to_string(),
            rules: LevelSpec::default(),
            content: TestContent {
                shape: "octagon".to_string(),
                size: 8,
            },
        };
        let text = serde_json::to_string(&level).expect("serialize");
        let parsed: LevelConfig<TestContent> = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, level);
    }

    #[test]
    fn level_index_parses_only_level_json_names() {
        assert_eq!(level_index(Path::new("level_0.json")), Some(0));
        assert_eq!(
            level_index(Path::new("/packs/easy/level_12.json")),
            Some(12)
        );
        assert_eq!(level_index(Path::new("level_.json")), None);
        assert_eq!(level_index(Path::new("level_x.json")), None);
        assert_eq!(level_index(Path::new("level_1.toml")), None);
        assert_eq!(level_index(Path::new("readme.json")), None);
    }

    #[test]
    fn load_levels_dir_reads_files_in_index_order() {
        let dir = unique_temp_dir("ratgames_levels_order");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        // Written out of order; a non-level file is present and must be ignored.
        write_level(&dir, 2, "TWO");
        write_level(&dir, 0, "ZERO");
        write_level(&dir, 1, "ONE");
        std::fs::write(dir.join("README.txt"), "ignore me").expect("write note");

        let levels = load_levels_dir::<TestContent>(&dir).expect("load levels");
        let names: Vec<&str> = levels.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["ZERO", "ONE", "TWO"]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_levels_dir_errs_on_a_dir_without_levels() {
        let dir = unique_temp_dir("ratgames_levels_empty");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("README.txt"), "no levels here").expect("write note");

        assert!(matches!(
            load_levels_dir::<TestContent>(&dir),
            Err(LevelLoadError::NoLevels { .. })
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_levels_dir_errs_on_a_missing_dir() {
        let dir = unique_temp_dir("ratgames_levels_missing");
        // Never created.
        assert!(matches!(
            load_levels_dir::<TestContent>(&dir),
            Err(LevelLoadError::Io { .. })
        ));
    }

    #[test]
    fn take_levels_flag_extracts_dir_and_keeps_the_rest_in_order() {
        let args = ["--levels", "packs/easy", "--config", "c.json", "HELLO"].map(String::from);
        let (dir, rest) = take_levels_flag(args).expect("parse");
        assert_eq!(dir, Some(PathBuf::from("packs/easy")));
        assert_eq!(rest, vec!["--config", "c.json", "HELLO"]);
    }

    #[test]
    fn take_levels_flag_accepts_the_equals_form() {
        let (dir, rest) = take_levels_flag(["--levels=packs/hard".to_string()]).expect("parse");
        assert_eq!(dir, Some(PathBuf::from("packs/hard")));
        assert!(rest.is_empty());
    }

    #[test]
    fn take_levels_flag_requires_a_dir() {
        assert!(matches!(
            take_levels_flag(["--levels".to_string()]),
            Err(LevelLoadError::MissingDir)
        ));
    }

    #[test]
    fn take_levels_flag_passes_everything_through_when_absent() {
        let (dir, rest) =
            take_levels_flag(["--config".to_string(), "c.json".to_string()]).expect("parse");
        assert_eq!(dir, None);
        assert_eq!(rest, vec!["--config", "c.json"]);
    }
}

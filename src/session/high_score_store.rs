//! JSON file persistence for a [`HighScores`] board — the filesystem adapter.
//!
//! [`HighScores`] is a pure value type that knows nothing of disk;
//! [`JsonHighScoreStore`] is where it meets the filesystem. It binds a file path
//! once and reads or writes the board as a small JSON array (the board's own
//! [`serde`] shape).
//!
//! A store is the *mechanism*: it reports IO and parse failures as
//! [`HighScoreStoreError`] and never logs or swallows them — a consumer layers
//! its own policy (warn and continue, fall back to an empty board) on top. The
//! one behaviour baked in is universal rather than policy: a **missing** file is
//! an empty board, since a board that has never been saved simply has no entries
//! yet. A present-but-unreadable or malformed file *is* an error, so a caller can
//! still tell "first run" apart from "the save file is corrupt."

use std::path::{Path, PathBuf};

use super::HighScores;

/// Errors reading or writing a [`JsonHighScoreStore`]'s file. Each carries the
/// path so a caller's message can name the offending file.
#[derive(Debug, thiserror::Error)]
pub enum HighScoreStoreError {
    /// The file exists but could not be read.
    #[error("failed to read high scores {path:?}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    /// The file was read but its contents are not a valid board.
    #[error("failed to parse high scores {path:?}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// The board could not be written.
    #[error("failed to write high scores {path:?}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// A JSON file a [`HighScores`] board is loaded from and saved to.
///
/// Binds the path once so a caller need not thread it through every call. The
/// store holds no board of its own — [`load`](Self::load) returns one and
/// [`save`](Self::save) writes one back — so a game keeps a single in-memory
/// board and persists it through the store as runs place.
#[derive(Debug, Clone)]
pub struct JsonHighScoreStore {
    path: PathBuf,
}

impl JsonHighScoreStore {
    /// A store backed by the file at `path`. The file need not exist yet; it is
    /// created on the first successful [`save`](Self::save).
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The file this store reads and writes.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load the board. A missing file is an empty board (the first run); a
    /// present-but-unreadable or malformed file is an error.
    ///
    /// # Errors
    /// [`HighScoreStoreError::Read`] if the file exists but cannot be read;
    /// [`HighScoreStoreError::Parse`] if its contents are not a valid board.
    pub fn load(&self) -> Result<HighScores, HighScoreStoreError> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|source| HighScoreStoreError::Parse {
                path: self.path.clone(),
                source,
            }),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(HighScores::new()),
            Err(source) => Err(HighScoreStoreError::Read {
                path: self.path.clone(),
                source,
            }),
        }
    }

    /// Write `board` to the file as pretty JSON, replacing any previous contents.
    ///
    /// # Errors
    /// [`HighScoreStoreError::Write`] if the file cannot be written.
    pub fn save(&self, board: &HighScores) -> Result<(), HighScoreStoreError> {
        // A board is a flat array of name/points entries — no maps, no floats —
        // so serialising it cannot fail; only the write can.
        let json = serde_json::to_string_pretty(board).expect("high scores serialise cleanly");
        std::fs::write(&self.path, json).map_err(|source| HighScoreStoreError::Write {
            path: self.path.clone(),
            source,
        })
    }
}

/// A serde config for a high-score board: how many places it keeps (the "top N")
/// and the file it persists to. A game carries the product values in its config,
/// passing `capacity` to [`HighScores::record`](super::HighScores::record) and
/// building the store from `file` with [`store`](Self::store) — the reusable
/// *type* lives here, the *values* live in the game's config, like
/// [`CountdownConfig`](crate::CountdownConfig).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ScoresConfig {
    /// Maximum entries kept on the board (the "top N").
    pub capacity: usize,
    /// File the board is persisted to, relative to the working directory.
    pub file: PathBuf,
}

impl Default for ScoresConfig {
    fn default() -> Self {
        // A neutral top-ten board in a generically-named file; a game carries its
        // own product name (e.g. "mathgame-highscores.json") in its config.
        Self {
            capacity: 10,
            file: PathBuf::from("highscores.json"),
        }
    }
}

impl ScoresConfig {
    /// A [`JsonHighScoreStore`] bound to this config's `file`.
    #[must_use]
    pub fn store(&self) -> JsonHighScoreStore {
        JsonHighScoreStore::new(&self.file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store on a unique temp path (per process and tag), with any stale file
    /// from a prior run removed, so the suite never collides or touches a real
    /// board.
    fn temp_store(tag: &str) -> JsonHighScoreStore {
        let path = std::env::temp_dir().join(format!(
            "ratgames-high-scores-test-{}-{tag}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        JsonHighScoreStore::new(path)
    }

    fn board(pairs: &[(&str, u32)]) -> HighScores {
        let mut b = HighScores::new();
        for (name, points) in pairs {
            b.record(*name, *points, 10);
        }
        b
    }

    #[test]
    fn a_missing_file_loads_an_empty_board() {
        let store = temp_store("missing");
        let loaded = store
            .load()
            .expect("a missing file must be an empty board, not an error");
        assert!(loaded.is_empty());
    }

    #[test]
    fn save_then_load_round_trips() {
        let store = temp_store("roundtrip");
        let b = board(&[("ADA", 300), ("GRACE", 100)]);
        store.save(&b).expect("save");
        assert_eq!(store.load().expect("load"), b);
        let _ = std::fs::remove_file(store.path());
    }

    #[test]
    fn save_overwrites_a_previous_board() {
        let store = temp_store("overwrite");
        store.save(&board(&[("OLD", 100)])).expect("first save");
        let newer = board(&[("NEW", 500)]);
        store.save(&newer).expect("second save");
        assert_eq!(store.load().expect("load"), newer);
        let _ = std::fs::remove_file(store.path());
    }

    #[test]
    fn a_malformed_file_is_a_parse_error() {
        let store = temp_store("malformed");
        std::fs::write(store.path(), "not json").expect("write garbage");
        assert!(matches!(
            store.load(),
            Err(HighScoreStoreError::Parse { .. })
        ));
        let _ = std::fs::remove_file(store.path());
    }

    #[test]
    fn the_error_message_names_the_file() {
        let store = temp_store("named");
        std::fs::write(store.path(), "not json").expect("write garbage");
        let message = store.load().unwrap_err().to_string();
        assert!(
            message.contains("named"),
            "message should name the file: {message}"
        );
        let _ = std::fs::remove_file(store.path());
    }

    #[test]
    fn scores_config_builds_a_store_and_round_trips() {
        let config = ScoresConfig {
            capacity: 5,
            file: PathBuf::from("board.json"),
        };
        assert_eq!(config.store().path(), Path::new("board.json"));

        let text = serde_json::to_string(&config).expect("serialize");
        let parsed: ScoresConfig = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, config);
        // A sparse config fills every field from the (generic) default.
        let defaulted: ScoresConfig = serde_json::from_str("{}").expect("deserialize empty");
        assert_eq!(defaulted, ScoresConfig::default());
        assert_eq!(defaulted.file, PathBuf::from("highscores.json"));
    }
}

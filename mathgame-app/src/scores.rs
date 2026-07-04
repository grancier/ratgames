//! File persistence for the high-score board — the storage adapter the app owns.
//!
//! `ratgames::HighScores` is a pure value type; this is where it meets disk. The
//! board is a small JSON array, read on startup and rewritten whenever a run
//! places. Saving is best-effort: a read-only or full disk should never crash
//! the game, so [`record_and_save`] logs a write failure rather than propagating
//! it.

use std::path::Path;

use ratgames::HighScores;

/// Errors reading or writing the high-score file.
#[derive(Debug, thiserror::Error)]
pub enum ScoresIoError {
    #[error("failed to read high scores {path:?}: {source}")]
    Read {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse high scores {path:?}: {source}")]
    Parse {
        path: std::path::PathBuf,
        source: serde_json::Error,
    },
    #[error("failed to write high scores {path:?}: {source}")]
    Write {
        path: std::path::PathBuf,
        source: std::io::Error,
    },
}

/// Load the board from `path`. A missing file is an empty board (the first run);
/// a present-but-unreadable or malformed file is an error.
pub fn load(path: &Path) -> Result<HighScores, ScoresIoError> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|source| ScoresIoError::Parse {
            path: path.to_path_buf(),
            source,
        }),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(HighScores::new()),
        Err(source) => Err(ScoresIoError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Write the board to `path` as pretty JSON.
pub fn save(scores: &HighScores, path: &Path) -> Result<(), ScoresIoError> {
    // Serialising a list of name/points entries cannot fail (no maps, no floats).
    let json = serde_json::to_string_pretty(scores).expect("high scores serialise cleanly");
    std::fs::write(path, json).map_err(|source| ScoresIoError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Record `name`/`points` on `board` (capped at `capacity`) and persist it to
/// `path`. A save failure is non-fatal: it is logged and swallowed so a run's
/// score is still reflected in-memory for the results screen.
pub fn record_and_save(
    board: &mut HighScores,
    name: &str,
    points: u32,
    capacity: usize,
    path: &Path,
) {
    board.record(name, points, capacity);
    if let Err(error) = save(board, path) {
        eprintln!("warning: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique temp path per test, so the suite never touches the real board and
    /// tests do not collide.
    fn temp_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "mathgame-scores-test-{}-{tag}.json",
            std::process::id()
        ))
    }

    #[test]
    fn a_missing_file_loads_an_empty_board() {
        let path = temp_path("missing");
        let _ = std::fs::remove_file(&path); // ensure absent
        let board = load(&path).expect("missing file must be an empty board");
        assert!(board.is_empty());
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_path("roundtrip");
        let mut board = HighScores::new();
        board.record("ADA", 300, 10);
        board.record("GRACE", 100, 10);
        save(&board, &path).expect("save");

        let loaded = load(&path).expect("load");
        assert_eq!(loaded, board);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_malformed_file_is_an_error() {
        let path = temp_path("malformed");
        std::fs::write(&path, "not json").expect("write garbage");
        assert!(matches!(load(&path), Err(ScoresIoError::Parse { .. })));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn record_and_save_updates_the_board_and_the_file() {
        let path = temp_path("record");
        let _ = std::fs::remove_file(&path);
        let mut board = HighScores::new();

        record_and_save(&mut board, "ZOE", 250, 10, &path);

        assert_eq!(board.entries().len(), 1);
        assert_eq!(board.entries()[0].points, 250);
        assert_eq!(load(&path).expect("reload"), board);
        let _ = std::fs::remove_file(&path);
    }
}

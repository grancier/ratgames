//! The app's high-score persistence *policy*, layered over ratgames' JSON store.
//!
//! [`ratgames::JsonHighScoreStore`] is the mechanism — it loads and saves a
//! `HighScores` board and reports failures. This module adds the app's policy:
//! persistence is best-effort, so a read-only or full disk warns rather than
//! crashes the game. A missing board file is already an empty board (the first
//! run); a *corrupt* one is a load error we also warn past, starting fresh.

use ratgames::{HighScores, JsonHighScoreStore};

/// Load the board, warning past a corrupt or unreadable file and starting
/// empty. A missing file is already an empty board (not an error), so only a
/// genuine read/parse failure trips the warning.
pub fn load_or_warn(store: &JsonHighScoreStore) -> HighScores {
    store.load().unwrap_or_else(|error| {
        eprintln!("warning: {error}; starting with an empty high-score board");
        HighScores::new()
    })
}

/// Record `name`/`points` on `board` (capped at `capacity`) and persist it
/// through `store`. A save failure is non-fatal: it is logged and swallowed so
/// the run's score is still reflected in-memory for the results screen.
pub fn record_and_save(
    store: &JsonHighScoreStore,
    board: &mut HighScores,
    name: &str,
    points: u32,
    capacity: usize,
) {
    board.record(name, points, capacity);
    if let Err(error) = store.save(board) {
        eprintln!("warning: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store on a unique temp path (per process and tag), with any stale
    /// file removed, so the suite never collides or touches the real board.
    fn temp_store(tag: &str) -> JsonHighScoreStore {
        let path = std::env::temp_dir().join(format!(
            "wordgame-scores-policy-test-{}-{tag}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        JsonHighScoreStore::new(path)
    }

    #[test]
    fn a_corrupt_file_warns_and_starts_empty() {
        // The policy that keeps a broken save file from crashing the game: a
        // load error becomes a fresh board, not a propagated failure.
        let store = temp_store("corrupt");
        std::fs::write(store.path(), "not json").expect("write garbage");
        assert!(load_or_warn(&store).is_empty());
        let _ = std::fs::remove_file(store.path());
    }

    #[test]
    fn record_and_save_updates_the_board_and_the_file() {
        let store = temp_store("record");
        let mut board = HighScores::new();

        record_and_save(&store, &mut board, "ZOE", 250, 10);

        assert_eq!(board.entries().len(), 1);
        assert_eq!(board.entries()[0].points, 250);
        assert_eq!(store.load().expect("reload"), board);
        let _ = std::fs::remove_file(store.path());
    }
}

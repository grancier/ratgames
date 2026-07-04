//! A ranked high-score table — generic arcade machinery.
//!
//! [`HighScores`] keeps name/points entries sorted best-first and capped at a
//! caller-supplied capacity. It is a pure value type: no windowing, no math, no
//! filesystem. A game records a run's outcome with [`record`](HighScores::record)
//! and persists the table however it likes — the [`serde`] derives serialise it
//! as a plain array of entries, so a save file is just that array.
//!
//! The capacity is a caller (config) concern, passed to each mutation rather than
//! stored, so a saved board never pins the game to the capacity it happened to be
//! recorded with.

/// One row of a [`HighScores`] board: a player's name and the points scored.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HighScoreEntry {
    pub name: String,
    pub points: u32,
}

/// A ranked high-score table, highest points first.
///
/// Serialises transparently as its array of [`HighScoreEntry`]: the capacity is
/// policy applied when recording, not part of the stored data.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct HighScores {
    entries: Vec<HighScoreEntry>,
}

impl HighScores {
    /// An empty board.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The ranked entries, highest first.
    #[must_use]
    pub fn entries(&self) -> &[HighScoreEntry] {
        &self.entries
    }

    /// How many entries are on the board.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the board has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Whether `points` would earn a place on a board of `capacity`: the board
    /// still has room, or `points` strictly beats the lowest-ranked entry.
    /// `capacity == 0` never qualifies.
    #[must_use]
    pub fn qualifies(&self, points: u32, capacity: usize) -> bool {
        if capacity == 0 {
            return false;
        }
        if self.entries.len() < capacity {
            return true;
        }
        // Board full: only a score above the current lowest earns a place.
        self.entries
            .last()
            .is_some_and(|lowest| points > lowest.points)
    }

    /// Record `name`/`points`, keeping the board sorted highest-first and capped
    /// at `capacity`. Returns the new entry's rank (0-based) if it placed, or
    /// `None` if it did not qualify (a full board it does not beat, or
    /// `capacity == 0`) — in which case the board is left unchanged.
    ///
    /// Ties are stable: an equal score ranks *below* the entries already there,
    /// so a returning tie never displaces an incumbent.
    pub fn record(
        &mut self,
        name: impl Into<String>,
        points: u32,
        capacity: usize,
    ) -> Option<usize> {
        if !self.qualifies(points, capacity) {
            return None;
        }
        // Insert before the first entry with strictly fewer points; equal scores
        // fall after the incumbents (stable ties).
        let rank = self
            .entries
            .iter()
            .position(|e| e.points < points)
            .unwrap_or(self.entries.len());
        self.entries.insert(
            rank,
            HighScoreEntry {
                name: name.into(),
                points,
            },
        );
        // A qualifying insert always lands within the top `capacity`, so the
        // truncation only ever drops the entry the new one displaced.
        self.entries.truncate(capacity);
        Some(rank)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a board by recording each pair under a generous cap.
    fn board(pairs: &[(&str, u32)]) -> HighScores {
        let mut b = HighScores::new();
        for (name, points) in pairs {
            b.record(*name, *points, 10);
        }
        b
    }

    fn names(board: &HighScores) -> Vec<&str> {
        board.entries().iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn new_board_is_empty() {
        let b = HighScores::new();
        assert!(b.is_empty());
        assert_eq!(b.len(), 0);
        assert_eq!(b.entries(), &[]);
    }

    #[test]
    fn records_keep_highest_first() {
        let b = board(&[("A", 100), ("B", 300), ("C", 200)]);
        assert_eq!(names(&b), ["B", "C", "A"]);
    }

    #[test]
    fn record_returns_the_rank_it_landed_at() {
        let mut b = board(&[("A", 100), ("B", 300)]); // [B300, A100]
        assert_eq!(b.record("C", 200, 10), Some(1)); // between B and A
        assert_eq!(b.record("D", 50, 10), Some(3)); // last
        assert_eq!(b.record("E", 400, 10), Some(0)); // new top
    }

    #[test]
    fn capacity_caps_the_board_and_drops_the_lowest() {
        let mut b = board(&[("A", 100), ("B", 200), ("C", 300)]); // [C300, B200, A100]
        assert_eq!(b.record("D", 250, 3), Some(1)); // bumps A(100)
        assert_eq!(b.len(), 3);
        assert_eq!(names(&b), ["C", "D", "B"]);
    }

    #[test]
    fn a_score_below_a_full_board_does_not_place() {
        let mut b = board(&[("A", 100), ("B", 200), ("C", 300)]);
        assert!(!b.qualifies(50, 3));
        assert_eq!(b.record("D", 50, 3), None);
        assert_eq!(b.len(), 3); // unchanged
        assert_eq!(names(&b), ["C", "B", "A"]);
    }

    #[test]
    fn qualifies_covers_room_full_ties_and_zero_capacity() {
        let full = board(&[("A", 100), ("B", 200), ("C", 300)]);
        assert!(full.qualifies(150, 5)); // room to spare
        assert!(full.qualifies(150, 3)); // full, but beats the lowest (100)
        assert!(!full.qualifies(100, 3)); // ties the lowest — must beat it
        assert!(!full.qualifies(50, 3)); // below the lowest
        assert!(!full.qualifies(9999, 0)); // no room at all
    }

    #[test]
    fn ties_rank_below_the_incumbent() {
        let mut b = board(&[("A", 200)]);
        assert_eq!(b.record("B", 200, 10), Some(1)); // below the existing 200
        assert_eq!(names(&b), ["A", "B"]);
    }

    #[test]
    fn serialises_as_a_flat_array_and_round_trips() {
        let b = board(&[("ADA", 300), ("GRACE", 100)]);
        let json = serde_json::to_string(&b).unwrap();
        assert_eq!(
            json,
            r#"[{"name":"ADA","points":300},{"name":"GRACE","points":100}]"#
        );
        let back: HighScores = serde_json::from_str(&json).unwrap();
        assert_eq!(back, b);
    }
}

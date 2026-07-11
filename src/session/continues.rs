//! The arcade continue: how many times a game-over run may resume, and what it
//! keeps.
//!
//! Like [`ScoringRules`](super::ScoringRules) this is a reusable, math-free
//! *rules* type a game deserialises its own values into — the type lives here,
//! the product values live in a game's config. The mechanism that applies it
//! (the used-continue counter, refilled lives, the re-armed level) lives on
//! [`GameRun`](super::GameRun); this is only the tunables it reads.
//!
//! The [`Default`] is a deliberate no-op — no continues — so a run left
//! unconfigured ends exactly as it did before continues existed. A game opts in
//! with [`GameRun::set_continues`](super::GameRun::set_continues).

/// A game's continue policy: how many continues a playthrough may use, and
/// whether the score survives one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ContinueRules {
    /// Continues a playthrough may use. `0` (the default) offers none.
    pub allowed: u32,
    /// Whether the score survives a continue. The arcade classic zeroes it
    /// (`false`, the default); `true` keeps it.
    pub keep_score: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_offers_no_continues() {
        let rules = ContinueRules::default();
        assert_eq!(rules.allowed, 0);
        assert!(!rules.keep_score);
    }

    #[test]
    fn rules_round_trip_through_json() {
        let rules = ContinueRules {
            allowed: 2,
            keep_score: true,
        };
        let text = serde_json::to_string(&rules).expect("serialize");
        let parsed: ContinueRules = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, rules);

        // A sparse config fills the rest from the no-op default.
        let sparse: ContinueRules = serde_json::from_str(r#"{"allowed":1}"#).expect("sparse");
        assert_eq!(sparse.allowed, 1);
        assert!(!sparse.keep_score);
    }
}

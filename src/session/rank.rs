//! Rank-based endings: the ordered rules that turn a finished run's facts into an
//! arcade ending title ("MATH MASTER", "NO MISS CHAMP", …).
//!
//! Like [`ScoringRules`](super::ScoringRules) these are reusable, math-free
//! *rules* a game deserialises its own values into — the type lives here, the
//! product titles and thresholds live in a game's config. Unlike scoring, ranking
//! is a pure end-of-run *read*: it needs no per-run state, so [`GameRun`]
//! (super::GameRun) does not hold a copy — a game passes its rules to
//! [`GameRun::rank`](super::GameRun::rank) (or evaluates [`RankRules::rank`]
//! directly) when the run ends.
//!
//! The [`Default`] is a deliberate no-op — no rules, so every run ranks as `None`
//! and a game falls back to its plain win / game-over title.

use super::RunTally;

/// One rank: a title awarded when a finished run meets every requirement. Fields
/// left at their defaults require nothing, so a rule with only a `title` is a
/// catch-all.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RankRule {
    /// The ending title this rank awards (e.g. `"NO MISS CHAMP"`).
    pub title: String,
    /// Whether the run must have been won (every level cleared).
    pub requires_won: bool,
    /// The least points the run must have scored.
    pub min_points: u32,
    /// The most failed attempts the run may have made across the whole
    /// playthrough (`Some(0)` = flawless). `None` requires nothing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_failures: Option<u32>,
}

impl RankRule {
    /// Whether a run with these facts earns this rank.
    fn matches(&self, won: bool, tally: RunTally, points: u32) -> bool {
        if self.requires_won && !won {
            return false;
        }
        if points < self.min_points {
            return false;
        }
        if let Some(max) = self.max_failures
            && tally.failures > max
        {
            return false;
        }
        true
    }
}

/// A game's whole ranking policy: rules tried in order, first match wins — so a
/// game lists its proudest rank first and (optionally) ends with a catch-all.
///
/// A game deserialises its own values into this; the [`Default`] is a no-op (no
/// rules — see the module docs). Serialises as the bare rule list.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct RankRules {
    /// The ranks, proudest first.
    pub rules: Vec<RankRule>,
}

/// Why a [`RankRules`] was rejected as malformed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RankRulesError {
    /// A rule's title was empty — it would award a blank ending.
    #[error("rank rule {index}: the title must not be empty")]
    EmptyTitle { index: usize },
}

impl RankRules {
    /// Check the policy is well-formed: every rule has a title.
    ///
    /// # Errors
    /// [`RankRulesError`] naming the first problem found.
    pub fn validate(&self) -> Result<(), RankRulesError> {
        for (index, rule) in self.rules.iter().enumerate() {
            if rule.title.is_empty() {
                return Err(RankRulesError::EmptyTitle { index });
            }
        }
        Ok(())
    }

    /// The ending title for a run that finished with these facts: the first rule
    /// every requirement of which the run meets, or `None` when no rule matches
    /// (the game falls back to its plain ending).
    #[must_use]
    pub fn rank(&self, won: bool, tally: RunTally, points: u32) -> Option<&str> {
        self.rules
            .iter()
            .find(|rule| rule.matches(won, tally, points))
            .map(|rule| rule.title.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(successes: u32, failures: u32) -> RunTally {
        RunTally {
            successes,
            failures,
        }
    }

    /// A proud-first policy: flawless win, then any win, then a scored catch-all.
    fn policy() -> RankRules {
        RankRules {
            rules: vec![
                RankRule {
                    title: "NO MISS CHAMP".to_string(),
                    requires_won: true,
                    max_failures: Some(0),
                    ..Default::default()
                },
                RankRule {
                    title: "MATH MASTER".to_string(),
                    requires_won: true,
                    ..Default::default()
                },
                RankRule {
                    title: "GOOD EFFORT".to_string(),
                    min_points: 1000,
                    ..Default::default()
                },
            ],
        }
    }

    #[test]
    fn default_is_a_no_op_policy() {
        assert_eq!(RankRules::default().rank(true, tally(20, 0), 9999), None);
    }

    #[test]
    fn the_first_matching_rule_wins() {
        let rules = policy();
        // A flawless win earns the proudest rank, not the later ones it also meets.
        assert_eq!(rules.rank(true, tally(20, 0), 5000), Some("NO MISS CHAMP"));
        // A blemished win falls through to the plain win rank.
        assert_eq!(rules.rank(true, tally(20, 2), 5000), Some("MATH MASTER"));
    }

    #[test]
    fn requires_won_gates_a_lost_run() {
        let rules = policy();
        // A lost run skips both win ranks; a big enough score still ranks.
        assert_eq!(rules.rank(false, tally(15, 6), 1500), Some("GOOD EFFORT"));
        // Below the catch-all's points, nothing matches.
        assert_eq!(rules.rank(false, tally(3, 6), 300), None);
    }

    #[test]
    fn min_points_gates_a_low_score() {
        let rules = RankRules {
            rules: vec![RankRule {
                title: "BIG SCORE".to_string(),
                min_points: 1000,
                ..Default::default()
            }],
        };
        assert_eq!(rules.rank(false, tally(0, 0), 999), None);
        assert_eq!(rules.rank(false, tally(0, 0), 1000), Some("BIG SCORE"));
    }

    #[test]
    fn max_failures_tolerates_up_to_the_limit() {
        let rules = RankRules {
            rules: vec![RankRule {
                title: "STEADY HAND".to_string(),
                max_failures: Some(2),
                ..Default::default()
            }],
        };
        assert_eq!(rules.rank(false, tally(9, 2), 0), Some("STEADY HAND"));
        assert_eq!(rules.rank(false, tally(9, 3), 0), None);
    }

    #[test]
    fn validate_rejects_an_empty_title() {
        let rules = RankRules {
            rules: vec![
                RankRule {
                    title: "FINE".to_string(),
                    ..Default::default()
                },
                RankRule::default(),
            ],
        };
        assert_eq!(
            rules.validate(),
            Err(RankRulesError::EmptyTitle { index: 1 })
        );
        assert!(policy().validate().is_ok());
        assert!(RankRules::default().validate().is_ok());
    }

    #[test]
    fn rules_round_trip_through_json_as_a_bare_list() {
        let rules = policy();
        let text = serde_json::to_string(&rules).expect("serialize");
        assert!(text.starts_with('['), "transparent: a bare rule list");
        let parsed: RankRules = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, rules);

        // A title-only rule fills the rest from defaults.
        let sparse: RankRule =
            serde_json::from_str(r#"{"title":"MATH MASTER","requires_won":true}"#).expect("rule");
        assert_eq!(sparse.max_failures, None);
        assert_eq!(sparse.min_points, 0);
    }
}

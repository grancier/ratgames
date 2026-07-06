//! Arcade scoring policy: how a run rewards combos and clean levels, and when it
//! grants extra lives.
//!
//! Like [`GameRules`](super::GameRules) and [`LevelSpec`](super::LevelSpec) these
//! are reusable, math-free *rules* a game deserialises its own values into — the
//! type lives here, the product values live in a game's config. The mechanism
//! that applies them (counting the streak, detecting a perfect clear, crossing
//! 1UP thresholds, capping lives) lives on [`GameRun`](super::GameRun); this is
//! only the tunables it reads.
//!
//! The [`Default`] is a deliberate no-op — no combo bonus, no perfect bonus, no
//! 1UP thresholds, and no lives cap — so a run left unconfigured scores exactly
//! its per-level base points and behaves as it did before scoring existed. A game
//! opts into richer scoring by supplying its own [`ScoringRules`].

/// How a run rewards consecutive correct answers (a combo). The [`Default`] is no
/// combo bonus (`bonus_per_step == 0`); a product opts in with its own value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct StreakRules {
    /// Points added per consecutive success beyond the first. A streak of `n`
    /// answers in a row awards `bonus_per_step * (n - 1)`, so the first correct
    /// answer earns no combo and each further one escalates. `0` disables the
    /// combo bonus.
    pub bonus_per_step: u32,
}

impl StreakRules {
    /// The combo bonus for a current run of `streak` consecutive successes. Zero
    /// for a streak of one (or zero) — the combo escalates from the second in a
    /// row. Saturating, so an implausibly long streak can never overflow.
    #[must_use]
    pub(super) fn bonus(self, streak: u32) -> u32 {
        self.bonus_per_step.saturating_mul(streak.saturating_sub(1))
    }
}

/// When a run grants extra lives, and the most it may hold.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct OneUpRules {
    /// The most lives a run may hold. A 1UP that would exceed this is forfeited,
    /// not banked. [`u32::MAX`] (the default) is effectively uncapped.
    pub max_lives: u32,
    /// Cumulative-score marks that each grant one extra life the first time the
    /// running score reaches them. Must be strictly ascending and non-zero (see
    /// [`ScoringRules::validate`]). Empty (the default) never grants a 1UP.
    pub thresholds: Vec<u32>,
}

impl Default for OneUpRules {
    fn default() -> Self {
        // No thresholds and no cap: a run neither earns nor is limited in lives
        // beyond what it starts with, matching the pre-scoring behaviour.
        Self {
            max_lives: u32::MAX,
            thresholds: Vec::new(),
        }
    }
}

/// A run's whole scoring policy: the combo bonus, the perfect-clear bonus, and the
/// 1UP thresholds with their lives cap.
///
/// A game deserialises its own values into this; the [`Default`] is a no-op (see
/// the module docs). [`GameRun::set_scoring`](super::GameRun::set_scoring) reads
/// it and applies the mechanism.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ScoringRules {
    /// Points for clearing a level with no failures (a perfect clear). `0`
    /// disables it.
    pub perfect_level_points: u32,
    /// The combo bonus.
    pub streak: StreakRules,
    /// The 1UP thresholds and lives cap.
    pub one_up: OneUpRules,
}

/// Why a [`ScoringRules`] was rejected as malformed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScoringRulesError {
    /// A 1UP threshold was zero — it would fire at the very start of a run.
    #[error("a 1UP threshold must be greater than zero")]
    ZeroThreshold,
    /// The 1UP thresholds were not strictly ascending — the run crosses them in
    /// order, so duplicates or descending marks are meaningless.
    #[error("1UP thresholds must be strictly ascending")]
    ThresholdsNotAscending,
    /// The lives cap is below the run's starting lives — a contradiction that
    /// would forbid every 1UP from the outset. Raised against a specific run by
    /// [`GameRun::set_scoring`](super::GameRun::set_scoring).
    #[error("max_lives ({max_lives}) must be at least starting_lives ({starting_lives})")]
    MaxLivesBelowStart { max_lives: u32, starting_lives: u32 },
}

impl ScoringRules {
    /// Check the policy is well-formed: every 1UP threshold is non-zero and the
    /// thresholds strictly ascend. The lives cap is checked against a run's
    /// starting lives separately, by
    /// [`GameRun::set_scoring`](super::GameRun::set_scoring).
    ///
    /// # Errors
    /// [`ScoringRulesError`] naming the first problem found.
    pub fn validate(&self) -> Result<(), ScoringRulesError> {
        let mut prev: Option<u32> = None;
        for &threshold in &self.one_up.thresholds {
            if threshold == 0 {
                return Err(ScoringRulesError::ZeroThreshold);
            }
            if let Some(previous) = prev
                && threshold <= previous
            {
                return Err(ScoringRulesError::ThresholdsNotAscending);
            }
            prev = Some(threshold);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_a_no_op_policy() {
        let rules = ScoringRules::default();
        assert_eq!(rules.perfect_level_points, 0);
        assert_eq!(rules.streak.bonus_per_step, 0);
        assert!(rules.one_up.thresholds.is_empty());
        assert_eq!(rules.one_up.max_lives, u32::MAX);
    }

    #[test]
    fn streak_bonus_escalates_from_the_second_in_a_row() {
        let streak = StreakRules { bonus_per_step: 25 };
        assert_eq!(streak.bonus(0), 0); // no streak
        assert_eq!(streak.bonus(1), 0); // first correct — no combo yet
        assert_eq!(streak.bonus(2), 25);
        assert_eq!(streak.bonus(3), 50);
        assert_eq!(streak.bonus(10), 225);
    }

    #[test]
    fn streak_bonus_of_zero_never_pays() {
        let streak = StreakRules::default();
        assert_eq!(streak.bonus(100), 0);
    }

    #[test]
    fn streak_bonus_saturates_instead_of_overflowing() {
        let streak = StreakRules {
            bonus_per_step: u32::MAX,
        };
        assert_eq!(streak.bonus(u32::MAX), u32::MAX);
    }

    #[test]
    fn validate_accepts_ascending_nonzero_thresholds() {
        let rules = ScoringRules {
            one_up: OneUpRules {
                max_lives: 5,
                thresholds: vec![1000, 5000, 12000],
            },
            ..Default::default()
        };
        assert!(rules.validate().is_ok());
    }

    #[test]
    fn validate_accepts_no_thresholds() {
        assert!(ScoringRules::default().validate().is_ok());
    }

    #[test]
    fn validate_rejects_a_zero_threshold() {
        let rules = ScoringRules {
            one_up: OneUpRules {
                max_lives: 5,
                thresholds: vec![0, 1000],
            },
            ..Default::default()
        };
        assert_eq!(rules.validate(), Err(ScoringRulesError::ZeroThreshold));
    }

    #[test]
    fn validate_rejects_non_ascending_thresholds() {
        let descending = ScoringRules {
            one_up: OneUpRules {
                max_lives: 5,
                thresholds: vec![5000, 1000],
            },
            ..Default::default()
        };
        assert_eq!(
            descending.validate(),
            Err(ScoringRulesError::ThresholdsNotAscending)
        );

        let duplicate = ScoringRules {
            one_up: OneUpRules {
                max_lives: 5,
                thresholds: vec![1000, 1000],
            },
            ..Default::default()
        };
        assert_eq!(
            duplicate.validate(),
            Err(ScoringRulesError::ThresholdsNotAscending)
        );
    }
}

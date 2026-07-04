//! The arcade run controller: the choreography that sequences a whole
//! playthrough, tying [`PlayerProfile`], [`Run`], and a per-level [`LevelGoal`]
//! together from a set of [`GameRules`].
//!
//! [`Run`] sequences the lives/level loop, [`LevelGoal`] judges one level, and
//! [`Score`](super::Score) counts points — but *someone* has to award points on a
//! win, feed the goal, advance or fail the run on the goal's verdict, and re-arm
//! a fresh goal for the next level. That glue lived in the game layer, hand-wired
//! and duplicated between "record an attempt" and "reset". [`GameRun`]
//! owns it, so a game supplies only the outcome of each attempt.
//!
//! The seam is a plain `bool`: [`record_attempt`](GameRun::record_attempt) takes
//! whether the attempt succeeded and returns an [`AttemptOutcome`]. No scoring
//! numbers, no notion of *what* was attempted — a quiz answer, a dodged enemy, a
//! cleared board — crosses in. The domain rules that decide success stay entirely
//! with the caller, so this never depends on any game's content.

use super::{LevelGoal, LevelOutcome, PlayerProfile, Run, RunPhase};

/// The tunables for a whole playthrough: how many lives and levels a run has, the
/// clear/fail goal each level repeats, and the points a success is worth.
///
/// A plain data type — construct it in code, or deserialise it from a game's
/// config. Every level uses the same goal (`required_successes` /
/// `max_failures`); escalating per-level difficulty is a future extension. The
/// [`Default`] is a sensible small arcade run, not a product's tuning — a game
/// carries its own values in config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GameRules {
    /// Lives a run starts with (and refills to on reset).
    pub starting_lives: u32,
    /// Number of levels to clear to win the run.
    pub total_levels: usize,
    /// Successes needed to clear a level.
    pub required_successes: u32,
    /// Failures tolerated per level before it fails (one more than this fails it).
    pub max_failures: u32,
    /// Points awarded for each successful attempt.
    pub points_per_success: u32,
}

impl Default for GameRules {
    fn default() -> Self {
        Self {
            starting_lives: 3,
            total_levels: 3,
            required_successes: 5,
            max_failures: 2,
            points_per_success: 100,
        }
    }
}

/// Why a set of [`GameRules`] was rejected: a degenerate value that would make a
/// run unplayable rather than merely hard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GameRulesError {
    /// `starting_lives` was zero — the run would be game over before it began.
    #[error("starting_lives must be at least 1")]
    ZeroLives,
    /// `total_levels` was zero — the run would be won before it began.
    #[error("total_levels must be at least 1")]
    ZeroLevels,
    /// `required_successes` was zero — a level with no way to be cleared.
    #[error("required_successes must be at least 1")]
    ZeroRequiredSuccesses,
}

impl GameRules {
    /// Check the run is playable: at least one life, one level, and one success
    /// required per level. `max_failures == 0` (fail on the first miss) and
    /// `points_per_success == 0` (a scoreless run) are permitted — strict, not
    /// broken.
    ///
    /// # Errors
    /// [`GameRulesError`] naming the first degenerate field found.
    pub fn validate(&self) -> Result<(), GameRulesError> {
        if self.starting_lives == 0 {
            return Err(GameRulesError::ZeroLives);
        }
        if self.total_levels == 0 {
            return Err(GameRulesError::ZeroLevels);
        }
        if self.required_successes == 0 {
            return Err(GameRulesError::ZeroRequiredSuccesses);
        }
        Ok(())
    }

    /// The per-level goal these rules describe. Private: a fresh goal to arm each
    /// level. Fails only on `required_successes == 0`, which [`validate`](Self::validate)
    /// already rejects, so a validated `GameRules` always yields one.
    fn level_goal(&self) -> Result<LevelGoal, GameRulesError> {
        LevelGoal::new(self.required_successes, self.max_failures)
            .map_err(|_| GameRulesError::ZeroRequiredSuccesses)
    }
}

/// What one recorded attempt did to a [`GameRun`]: how the current level now
/// stands, and where the run as a whole now stands.
///
/// The caller pairs this with its own domain detail (what the right answer was,
/// which enemy hit) to build whatever richer report it shows the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptOutcome {
    /// The current level's standing after the attempt.
    pub level_outcome: LevelOutcome,
    /// The run's standing after the attempt.
    pub run_phase: RunPhase,
}

/// One playthrough of a game: a [`PlayerProfile`], the arcade [`Run`], and the
/// current level's [`LevelGoal`], sequenced from a set of [`GameRules`].
///
/// Feed it the outcome of each attempt with [`record_attempt`](Self::record_attempt);
/// it awards points, judges the level, advances or fails the run, and re-arms the
/// goal for the next level. [`reset`](Self::reset) starts a fresh playthrough for
/// the same rules.
#[derive(Debug, Clone)]
pub struct GameRun {
    profile: PlayerProfile,
    run: Run,
    goal: LevelGoal,
    /// The validated goal to re-arm each new level from (a `Copy` of the initial
    /// goal), so re-arming never rebuilds — and so cannot fail — after construction.
    initial_goal: LevelGoal,
    points_per_success: u32,
}

impl GameRun {
    /// Start a playthrough under `rules`, with a default (nameless) profile.
    ///
    /// # Errors
    /// [`GameRulesError`] if `rules` are not playable (see [`GameRules::validate`]).
    pub fn new(rules: &GameRules) -> Result<Self, GameRulesError> {
        rules.validate()?;
        let goal = rules.level_goal()?;
        Ok(Self {
            profile: PlayerProfile::default(),
            run: Run::new(rules.starting_lives, rules.total_levels),
            goal,
            initial_goal: goal,
            points_per_success: rules.points_per_success,
        })
    }

    /// The player profile.
    #[must_use]
    pub fn profile(&self) -> &PlayerProfile {
        &self.profile
    }

    /// Set the player's name (e.g. after a name-entry screen).
    pub fn set_player_name(&mut self, name: impl Into<String>) {
        self.profile.set_name(name);
    }

    /// The arcade run (score, lives, level progress, phase).
    #[must_use]
    pub fn run(&self) -> Run {
        self.run
    }

    /// The current level's goal.
    #[must_use]
    pub fn goal(&self) -> LevelGoal {
        self.goal
    }

    /// Where the run stands right now.
    #[must_use]
    pub fn phase(&self) -> RunPhase {
        self.run.phase()
    }

    /// Record one attempt's outcome and sequence the run: award points on a
    /// success, feed the level goal, clear or fail the run on the goal's verdict,
    /// and re-arm a fresh goal when a level ends but the run continues.
    ///
    /// Once the run is over (won or game over) this is inert — it reports the
    /// current standing and changes nothing — so a late attempt cannot revive or
    /// re-score a finished run.
    pub fn record_attempt(&mut self, success: bool) -> AttemptOutcome {
        if self.run.phase() != RunPhase::Playing {
            return AttemptOutcome {
                level_outcome: self.goal.outcome(),
                run_phase: self.run.phase(),
            };
        }

        if success {
            self.run.award(self.points_per_success);
        }
        let level_outcome = self.goal.record(success);
        let run_phase = match level_outcome {
            LevelOutcome::InProgress => self.run.phase(),
            LevelOutcome::Cleared => self.run.clear_level(),
            LevelOutcome::Failed => self.run.fail(),
        };

        // A level ended (cleared or failed) but the run plays on: arm a fresh goal
        // for the next level.
        if run_phase == RunPhase::Playing && level_outcome != LevelOutcome::InProgress {
            self.goal = self.initial_goal;
        }

        AttemptOutcome {
            level_outcome,
            run_phase,
        }
    }

    /// Restart for a fresh playthrough: zero score, refilled lives, first level,
    /// and a fresh goal. The player profile is left intact.
    pub fn reset(&mut self) {
        self.run.reset();
        self.goal = self.initial_goal;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> GameRules {
        // A compact run to exercise the sequencing: 2 lives, 2 levels, cleared at
        // 3 successes, failed after 1 failure, 10 points a success.
        GameRules {
            starting_lives: 2,
            total_levels: 2,
            required_successes: 3,
            max_failures: 1,
            points_per_success: 10,
        }
    }

    #[test]
    fn game_run_names_the_reusable_run_controller() {
        let rules = GameRules {
            starting_lives: 1,
            total_levels: 1,
            required_successes: 2,
            max_failures: 0,
            points_per_success: 25,
        };
        let mut game_run = GameRun::new(&rules).unwrap();

        let first = game_run.record_attempt(true);
        assert_eq!(first.level_outcome, LevelOutcome::InProgress);
        assert_eq!(first.run_phase, RunPhase::Playing);
        assert_eq!(game_run.run().score().points(), 25);

        let second = game_run.record_attempt(true);
        assert_eq!(second.level_outcome, LevelOutcome::Cleared);
        assert_eq!(second.run_phase, RunPhase::Won);
        assert_eq!(game_run.run().score().points(), 50);
    }

    #[test]
    fn default_rules_are_playable() {
        assert!(GameRules::default().validate().is_ok());
    }

    #[test]
    fn validate_rejects_degenerate_rules() {
        let ok = GameRules::default();
        assert_eq!(
            GameRules {
                starting_lives: 0,
                ..ok
            }
            .validate(),
            Err(GameRulesError::ZeroLives)
        );
        assert_eq!(
            GameRules {
                total_levels: 0,
                ..ok
            }
            .validate(),
            Err(GameRulesError::ZeroLevels)
        );
        assert_eq!(
            GameRules {
                required_successes: 0,
                ..ok
            }
            .validate(),
            Err(GameRulesError::ZeroRequiredSuccesses)
        );
    }

    #[test]
    fn new_rejects_degenerate_rules() {
        assert_eq!(
            GameRun::new(&GameRules {
                required_successes: 0,
                ..GameRules::default()
            })
            .map(|_| ())
            .unwrap_err(),
            GameRulesError::ZeroRequiredSuccesses
        );
    }

    #[test]
    fn successes_clear_a_level_award_points_and_rearm_the_goal() {
        let mut game = GameRun::new(&rules()).unwrap();
        for _ in 0..2 {
            let out = game.record_attempt(true);
            assert_eq!(out.level_outcome, LevelOutcome::InProgress);
            assert_eq!(out.run_phase, RunPhase::Playing);
        }
        let out = game.record_attempt(true); // third success clears level 0
        assert_eq!(out.level_outcome, LevelOutcome::Cleared);
        assert_eq!(out.run_phase, RunPhase::Playing);
        assert_eq!(game.run().levels().current(), 1);
        assert_eq!(game.run().score().points(), 30); // 3 * 10
        // The goal was re-armed for the new level: counts back to zero.
        assert_eq!(game.goal().successes(), 0);
        assert_eq!(game.goal().failures(), 0);
    }

    #[test]
    fn exceeding_failures_fails_the_level_costs_a_life_and_rearms() {
        let mut game = GameRun::new(&rules()).unwrap();
        let first = game.record_attempt(false); // 1 failure == max, still in progress
        assert_eq!(first.level_outcome, LevelOutcome::InProgress);
        assert_eq!(game.run().lives().count(), 2);

        let out = game.record_attempt(false); // 2 > max: level failed
        assert_eq!(out.level_outcome, LevelOutcome::Failed);
        assert_eq!(out.run_phase, RunPhase::Playing);
        assert_eq!(game.run().lives().count(), 1); // cost a life
        assert_eq!(game.goal().failures(), 0); // re-armed
    }

    #[test]
    fn clearing_every_level_wins_the_run() {
        let mut game = GameRun::new(&rules()).unwrap();
        let mut last = None;
        for _ in 0..6 {
            // 2 levels * 3 successes
            last = Some(game.record_attempt(true));
        }
        let out = last.unwrap();
        assert_eq!(out.level_outcome, LevelOutcome::Cleared);
        assert_eq!(out.run_phase, RunPhase::Won);
        assert_eq!(game.run().score().points(), 60); // 6 * 10
    }

    #[test]
    fn running_out_of_lives_ends_the_run() {
        let mut game = GameRun::new(&rules()).unwrap();
        // Each level: 2 failures fail it and cost a life. 2 lives -> game over on
        // the second failed level.
        game.record_attempt(false);
        game.record_attempt(false); // level 0 failed, life 2 -> 1
        assert_eq!(game.phase(), RunPhase::Playing);
        game.record_attempt(false);
        let out = game.record_attempt(false); // level 1 failed, life 1 -> 0
        assert_eq!(out.run_phase, RunPhase::GameOver);
        assert_eq!(game.run().lives().count(), 0);
    }

    #[test]
    fn a_finished_run_ignores_further_attempts() {
        let mut game = GameRun::new(&rules()).unwrap();
        for _ in 0..6 {
            game.record_attempt(true); // win the run
        }
        assert_eq!(game.phase(), RunPhase::Won);
        let score = game.run().score().points();

        let out = game.record_attempt(true); // inert once won
        assert_eq!(out.run_phase, RunPhase::Won);
        assert_eq!(
            game.run().score().points(),
            score,
            "no points after the run ends"
        );
    }

    #[test]
    fn reset_restores_a_full_playthrough_and_keeps_the_name() {
        let mut game = GameRun::new(&rules()).unwrap();
        game.set_player_name("ADA");
        for _ in 0..6 {
            game.record_attempt(true); // win, so phase != Playing
        }
        assert_eq!(game.phase(), RunPhase::Won);

        game.reset();
        assert_eq!(game.phase(), RunPhase::Playing);
        assert_eq!(game.run().lives().count(), 2);
        assert_eq!(game.run().score().points(), 0);
        assert_eq!(game.run().levels().current(), 0);
        assert_eq!(game.goal().successes(), 0);
        assert_eq!(game.profile().name(), "ADA"); // profile survives a reset
    }
}

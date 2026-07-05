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
//!
//! A run is configured one of two ways: a uniform [`GameRules`] (every level
//! shares one goal), or a per-level [`Campaign`] — an ordered list of
//! [`LevelSpec`]s that lets each level carry its own goal, reward, and input
//! mode. Both build a [`GameRun`] that sequences the same loop.

use super::{LevelGoal, LevelOutcome, PlayerProfile, Run, RunPhase};
use crate::ui::{AnswerMode, AnswerModeError};

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

/// The rules for a single level: its clear/fail goal, the points a success is
/// worth, and how the player answers it.
///
/// A reusable, math-free description a game's per-level config deserialises into
/// — like [`GameRules`], the reusable *type* lives here while the product
/// *values* live in a game's config. A [`Campaign`] is an ordered list of these,
/// one per level, so each level can set its own goal, reward, and input mode —
/// which a single uniform [`GameRules`] cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LevelSpec {
    /// Successes needed to clear the level.
    pub required_successes: u32,
    /// Failures tolerated before the level fails (one more than this fails it).
    pub max_failures: u32,
    /// Points awarded for each successful attempt on this level.
    pub points_per_success: u32,
    /// Per-question time limit in frames (`0` = untimed). A question left
    /// unanswered when it elapses is a miss, exactly like a wrong answer.
    pub time_limit_frames: u32,
    /// How the player answers this level (typed, or a multiple-choice pick).
    pub answer_mode: AnswerMode,
}

impl Default for LevelSpec {
    fn default() -> Self {
        // A sensible small arcade level, matching the uniform `GameRules`
        // defaults; a game carries its own values in config.
        Self {
            required_successes: 5,
            max_failures: 2,
            points_per_success: 100,
            time_limit_frames: 0,
            answer_mode: AnswerMode::Typed,
        }
    }
}

/// Why a [`LevelSpec`] was rejected as unplayable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LevelSpecError {
    /// `required_successes` was zero — a level with no way to be cleared.
    #[error("required_successes must be at least 1")]
    ZeroRequiredSuccesses,
    /// The level's answer mode is itself unplayable.
    #[error("answer mode: {0}")]
    AnswerMode(#[from] AnswerModeError),
}

impl LevelSpec {
    /// Check the level is playable: at least one success required, and a playable
    /// answer mode. `max_failures == 0` (fail on the first miss) and
    /// `points_per_success == 0` (a scoreless level) are permitted.
    ///
    /// # Errors
    /// [`LevelSpecError`] naming the first problem found.
    pub fn validate(&self) -> Result<(), LevelSpecError> {
        if self.required_successes == 0 {
            return Err(LevelSpecError::ZeroRequiredSuccesses);
        }
        self.answer_mode.validate()?;
        Ok(())
    }

    /// A fresh goal for this level. Fails only on `required_successes == 0`, which
    /// [`validate`](Self::validate) already rejects.
    fn goal(&self) -> Result<LevelGoal, LevelSpecError> {
        LevelGoal::new(self.required_successes, self.max_failures)
            .map_err(|_| LevelSpecError::ZeroRequiredSuccesses)
    }
}

/// A whole playthrough as an ordered list of [`LevelSpec`]s plus the run-wide
/// starting lives — the per-level counterpart to a uniform [`GameRules`].
///
/// Build a [`GameRun`] from it with [`GameRun::from_campaign`]. Lives are
/// run-wide, not per level; the number of levels is `levels.len()`. Each level's
/// own goal is armed as the run reaches it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Campaign {
    /// Lives a run starts with (and refills to on reset).
    pub starting_lives: u32,
    /// The levels to clear, in order.
    pub levels: Vec<LevelSpec>,
}

impl Default for Campaign {
    fn default() -> Self {
        // A playable default: a three-level run of default levels, mirroring the
        // uniform `GameRules` default, so a default `Campaign` is a valid
        // playthrough rather than an empty (unplayable) one.
        Self {
            starting_lives: 3,
            levels: vec![LevelSpec::default(); 3],
        }
    }
}

/// Why a [`Campaign`] was rejected: a degenerate value that would make the run
/// unplayable rather than merely hard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CampaignError {
    /// `starting_lives` was zero — the run would be game over before it began.
    #[error("starting_lives must be at least 1")]
    ZeroLives,
    /// No levels — the run would be won before it began.
    #[error("a campaign must have at least one level")]
    NoLevels,
    /// A level was unplayable; `index` is its position in `levels`.
    #[error("level {index}: {source}")]
    Level {
        index: usize,
        source: LevelSpecError,
    },
}

impl Campaign {
    /// Check the campaign is playable: at least one life, at least one level, and
    /// every level playable.
    ///
    /// # Errors
    /// [`CampaignError`] naming the first degenerate value found.
    pub fn validate(&self) -> Result<(), CampaignError> {
        if self.starting_lives == 0 {
            return Err(CampaignError::ZeroLives);
        }
        if self.levels.is_empty() {
            return Err(CampaignError::NoLevels);
        }
        for (index, level) in self.levels.iter().enumerate() {
            level
                .validate()
                .map_err(|source| CampaignError::Level { index, source })?;
        }
        Ok(())
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
    /// The per-level specs, indexed by the current level. Non-empty and validated
    /// at construction — a uniform [`GameRules`] run expands to identical specs —
    /// so arming a level's goal by index never rebuilds an invalid one, and each
    /// level's points and input mode are read from its own spec.
    levels: Vec<LevelSpec>,
}

impl GameRun {
    /// Start a playthrough under `rules`, with a default (nameless) profile.
    ///
    /// # Errors
    /// [`GameRulesError`] if `rules` are not playable (see [`GameRules::validate`]).
    pub fn new(rules: &GameRules) -> Result<Self, GameRulesError> {
        rules.validate()?;
        let goal = rules.level_goal()?;
        // A uniform run is a campaign of identical levels: same goal, points, and
        // (typed) input on every one.
        let spec = LevelSpec {
            required_successes: rules.required_successes,
            max_failures: rules.max_failures,
            points_per_success: rules.points_per_success,
            time_limit_frames: 0,
            answer_mode: AnswerMode::Typed,
        };
        Ok(Self::assemble(
            PlayerProfile::default(),
            rules.starting_lives,
            vec![spec; rules.total_levels],
            goal,
        ))
    }

    /// Start a playthrough from a per-level [`Campaign`], with a default
    /// (nameless) profile. Each level's own goal, reward, and input mode apply as
    /// the run reaches it.
    ///
    /// # Errors
    /// [`CampaignError`] if the campaign is not playable (see
    /// [`Campaign::validate`]).
    pub fn from_campaign(campaign: &Campaign) -> Result<Self, CampaignError> {
        campaign.validate()?;
        // Validated above: at least one level, and level 0's goal builds.
        let goal = campaign.levels[0]
            .goal()
            .map_err(|source| CampaignError::Level { index: 0, source })?;
        Ok(Self::assemble(
            PlayerProfile::default(),
            campaign.starting_lives,
            campaign.levels.clone(),
            goal,
        ))
    }

    /// Assemble a run from already-validated parts: `levels` is non-empty and
    /// `goal` is the first level's freshly-armed goal.
    fn assemble(
        profile: PlayerProfile,
        starting_lives: u32,
        levels: Vec<LevelSpec>,
        goal: LevelGoal,
    ) -> Self {
        Self {
            run: Run::new(starting_lives, levels.len()),
            profile,
            goal,
            levels,
        }
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

        // Award the level being played — captured before a clear advances it.
        let current = self.run.levels().current();
        if success {
            self.run.award(self.levels[current].points_per_success);
        }
        let level_outcome = self.goal.record(success);
        let run_phase = match level_outcome {
            LevelOutcome::InProgress => self.run.phase(),
            LevelOutcome::Cleared => self.run.clear_level(),
            LevelOutcome::Failed => self.run.fail(),
        };

        // A level ended (cleared or failed) but the run plays on: arm the goal for
        // the level now current — the next one after a clear, or the same one to
        // retry after a fail.
        if run_phase == RunPhase::Playing && level_outcome != LevelOutcome::InProgress {
            self.goal = self.current_goal();
        }

        AttemptOutcome {
            level_outcome,
            run_phase,
        }
    }

    /// Award bonus points on top of a level's per-success reward — a time bonus, a
    /// streak or perfect-clear bonus. Unlike [`record_attempt`] this does no run
    /// sequencing; it simply adds points. The caller decides when the bonus is
    /// earned (typically on a success, including the one that wins the run).
    pub fn award(&mut self, points: u32) {
        self.run.award(points);
    }

    /// The spec of the level currently in play: its goal, reward, and input mode.
    /// Once the run is won (the level index has run past the last level) this
    /// reports the final level's spec.
    #[must_use]
    pub fn current_level_spec(&self) -> LevelSpec {
        let last = self.levels.len() - 1; // non-empty by construction
        self.levels[self.run.levels().current().min(last)]
    }

    /// Restart for a fresh playthrough: zero score, refilled lives, first level,
    /// and its goal re-armed. The player profile is left intact.
    pub fn reset(&mut self) {
        self.run.reset();
        self.goal = self.current_goal();
    }

    /// A fresh goal for the level now current. Infallible: the specs were
    /// validated at construction and the current index is in range wherever this
    /// is called (level 0 after a reset, or a level the run plays on).
    fn current_goal(&self) -> LevelGoal {
        self.levels[self.run.levels().current()]
            .goal()
            .expect("level specs validated at construction")
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
    fn award_adds_bonus_points_without_sequencing() {
        let mut game = GameRun::new(&rules()).unwrap();
        game.record_attempt(true); // 10 base points
        game.award(55); // a bonus, no run sequencing
        assert_eq!(game.run().score().points(), 65);

        // A bonus on the run-winning answer still counts: the win transitions the
        // run before the caller awards, and `award` is unguarded.
        let mut win = GameRun::new(&GameRules {
            starting_lives: 1,
            total_levels: 1,
            required_successes: 1,
            max_failures: 0,
            points_per_success: 100,
        })
        .unwrap();
        assert_eq!(win.record_attempt(true).run_phase, RunPhase::Won);
        win.award(25);
        assert_eq!(win.run().score().points(), 125);
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

    /// A two-level campaign whose levels differ in every reusable dimension:
    /// goal, reward, and input mode.
    fn campaign() -> Campaign {
        Campaign {
            starting_lives: 3,
            levels: vec![
                LevelSpec {
                    required_successes: 2,
                    max_failures: 1,
                    points_per_success: 10,
                    time_limit_frames: 0,
                    answer_mode: AnswerMode::Typed,
                },
                LevelSpec {
                    required_successes: 3,
                    max_failures: 2,
                    points_per_success: 100,
                    time_limit_frames: 300,
                    answer_mode: AnswerMode::MultipleChoice { options: 4 },
                },
            ],
        }
    }

    #[test]
    fn campaign_runs_each_levels_own_goal_reward_and_input() {
        let mut game = GameRun::from_campaign(&campaign()).unwrap();
        // Level 0: two successes at 10 points each clear it.
        assert_eq!(game.current_level_spec().answer_mode, AnswerMode::Typed);
        assert_eq!(
            game.record_attempt(true).level_outcome,
            LevelOutcome::InProgress
        );
        let cleared = game.record_attempt(true);
        assert_eq!(cleared.level_outcome, LevelOutcome::Cleared);
        assert_eq!(cleared.run_phase, RunPhase::Playing);
        assert_eq!(game.run().score().points(), 20);
        assert_eq!(game.run().levels().current(), 1);

        // Level 1 armed its own goal (3 successes) and its own input mode.
        assert_eq!(
            game.current_level_spec().answer_mode,
            AnswerMode::MultipleChoice { options: 4 }
        );
        assert_eq!(game.goal().required_successes(), 3);
        for _ in 0..2 {
            assert_eq!(
                game.record_attempt(true).level_outcome,
                LevelOutcome::InProgress
            );
        }
        let won = game.record_attempt(true); // third success clears the last level
        assert_eq!(won.level_outcome, LevelOutcome::Cleared);
        assert_eq!(won.run_phase, RunPhase::Won);
        // 2 * 10 (level 0) + 3 * 100 (level 1): rewards are per level.
        assert_eq!(game.run().score().points(), 320);
        // A won run still reports a spec (the final level's), not a panic.
        assert_eq!(
            game.current_level_spec().answer_mode,
            AnswerMode::MultipleChoice { options: 4 }
        );
    }

    #[test]
    fn failing_a_level_retries_the_same_levels_goal() {
        let mut game = GameRun::from_campaign(&campaign()).unwrap();
        // Level 0 tolerates one failure; the second fails the level.
        assert_eq!(
            game.record_attempt(false).level_outcome,
            LevelOutcome::InProgress
        );
        let failed = game.record_attempt(false);
        assert_eq!(failed.level_outcome, LevelOutcome::Failed);
        assert_eq!(failed.run_phase, RunPhase::Playing);
        assert_eq!(game.run().lives().count(), 2); // cost a life
        // Still on level 0, its goal freshly re-armed for a retry.
        assert_eq!(game.run().levels().current(), 0);
        assert_eq!(game.goal().failures(), 0);
        assert_eq!(game.goal().required_successes(), 2);
    }

    #[test]
    fn a_uniform_campaign_matches_equivalent_game_rules() {
        // new(&GameRules) is a campaign of identical levels, so a hand-built
        // uniform campaign sequences identically.
        let rules = GameRules {
            starting_lives: 2,
            total_levels: 2,
            required_successes: 3,
            max_failures: 1,
            points_per_success: 10,
        };
        let spec = LevelSpec {
            required_successes: 3,
            max_failures: 1,
            points_per_success: 10,
            time_limit_frames: 0,
            answer_mode: AnswerMode::Typed,
        };
        let uniform = Campaign {
            starting_lives: 2,
            levels: vec![spec, spec],
        };
        let mut from_rules = GameRun::new(&rules).unwrap();
        let mut from_campaign = GameRun::from_campaign(&uniform).unwrap();
        for _ in 0..6 {
            let a = from_rules.record_attempt(true);
            let b = from_campaign.record_attempt(true);
            assert_eq!(a, b);
        }
        assert_eq!(from_rules.phase(), RunPhase::Won);
        assert_eq!(
            from_rules.run().score().points(),
            from_campaign.run().score().points()
        );
    }

    #[test]
    fn default_campaign_is_playable() {
        assert!(Campaign::default().validate().is_ok());
        assert!(GameRun::from_campaign(&Campaign::default()).is_ok());
    }

    #[test]
    fn from_campaign_rejects_degenerate_campaigns() {
        let ok = LevelSpec::default();
        assert_eq!(
            Campaign {
                starting_lives: 0,
                levels: vec![ok],
            }
            .validate(),
            Err(CampaignError::ZeroLives)
        );
        assert_eq!(
            Campaign {
                starting_lives: 3,
                levels: vec![],
            }
            .validate(),
            Err(CampaignError::NoLevels)
        );
        // A degenerate level is reported with its index and the underlying reason.
        assert_eq!(
            Campaign {
                starting_lives: 3,
                levels: vec![
                    ok,
                    LevelSpec {
                        required_successes: 0,
                        ..ok
                    },
                ],
            }
            .validate(),
            Err(CampaignError::Level {
                index: 1,
                source: LevelSpecError::ZeroRequiredSuccesses,
            })
        );
        assert_eq!(
            Campaign {
                starting_lives: 3,
                levels: vec![LevelSpec {
                    answer_mode: AnswerMode::MultipleChoice { options: 1 },
                    ..ok
                }],
            }
            .validate(),
            Err(CampaignError::Level {
                index: 0,
                source: LevelSpecError::AnswerMode(AnswerModeError::TooFewOptions),
            })
        );
        // The constructor surfaces the same rejection.
        assert!(
            GameRun::from_campaign(&Campaign {
                starting_lives: 3,
                levels: vec![],
            })
            .is_err()
        );
    }

    #[test]
    fn level_spec_and_campaign_round_trip_through_json() {
        let campaign = campaign();
        let text = serde_json::to_string(&campaign).expect("serialize");
        let parsed: Campaign = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, campaign);
    }

    #[test]
    fn level_spec_defaults_fill_omitted_fields() {
        // `#[serde(default)]` draws omitted fields from the type's Default, so a
        // sparse level file is still a playable spec.
        let parsed: LevelSpec =
            serde_json::from_str(r#"{"required_successes":8}"#).expect("deserialize");
        assert_eq!(parsed.required_successes, 8);
        assert_eq!(parsed.max_failures, LevelSpec::default().max_failures);
        assert_eq!(parsed.time_limit_frames, 0); // omitted -> untimed
        assert_eq!(parsed.answer_mode, AnswerMode::Typed);
        assert!(parsed.validate().is_ok());
    }
}

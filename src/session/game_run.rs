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

use super::{
    LevelGoal, LevelOutcome, PlayerProfile, RankRules, Run, RunPhase, ScoringRules,
    ScoringRulesError,
};
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
/// stands, where the run as a whole now stands, and what scoring it earned.
///
/// The caller pairs this with its own domain detail (what the right answer was,
/// which enemy hit) to build whatever richer report it shows the player. The
/// scoring fields report what fired so a game can render it — a "STREAK ×5",
/// "PERFECT", or "1UP" flourish — without recomputing any of the policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttemptOutcome {
    /// The current level's standing after the attempt.
    pub level_outcome: LevelOutcome,
    /// The run's standing after the attempt.
    pub run_phase: RunPhase,
    /// Consecutive successes after this attempt: incremented on a success, reset
    /// to zero on any miss.
    pub streak: u32,
    /// Combo points awarded for the streak this attempt (zero on a miss, and on
    /// the first success of a streak).
    pub streak_bonus: u32,
    /// Points awarded for a perfect clear — the attempt cleared the level with no
    /// failures. Zero unless this attempt cleared the level cleanly.
    pub perfect_bonus: u32,
    /// Extra lives granted by 1UP thresholds the total award crossed this attempt
    /// (capped at `max_lives`; a forfeited 1UP is not counted).
    pub one_ups: u32,
}

/// What a bonus [`award`](GameRun::award) did to a [`GameRun`]: the 1UPs the added
/// points earned. Awarding does no run sequencing, so unlike an
/// [`AttemptOutcome`] there is no level or run standing to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AwardOutcome {
    /// Extra lives granted by 1UP thresholds this award crossed (capped at
    /// `max_lives`; a forfeited 1UP is not counted).
    pub one_ups: u32,
}

/// The run-long attempt tally: every success and failure recorded across the
/// whole playthrough, spanning levels (unlike the per-level [`LevelGoal`]
/// counts, which re-arm each level). Rank rules read it — a "no miss" ending
/// needs the playthrough's failures, which no per-level count survives to
/// report. Zeroed by [`reset`](GameRun::reset); a continue does not clear it (a
/// continued run is the same playthrough).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunTally {
    /// Successful attempts recorded this playthrough.
    pub successes: u32,
    /// Failed attempts (wrong answers and timeouts) recorded this playthrough.
    pub failures: u32,
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
    /// The scoring policy: combo, perfect-clear, and 1UP rules. A no-op
    /// [`ScoringRules::default`] until a game supplies its own with
    /// [`set_scoring`](Self::set_scoring), so an unconfigured run scores only its
    /// per-level base points.
    scoring: ScoringRules,
    /// Consecutive successes so far — the current combo length. Reset to zero on
    /// any miss and on [`reset`](Self::reset).
    streak: u32,
    /// How many 1UP thresholds have already been crossed. The thresholds ascend,
    /// so this is a cursor into `scoring.one_up.thresholds`: everything before it
    /// has fired, so each fires exactly once per playthrough.
    next_threshold: usize,
    /// The run-long success/failure tally, spanning levels — what rank rules
    /// judge a finished playthrough by.
    tally: RunTally,
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
            scoring: ScoringRules::default(),
            streak: 0,
            next_threshold: 0,
            tally: RunTally::default(),
        }
    }

    /// Set the scoring policy — the combo bonus, perfect-clear bonus, and 1UP
    /// thresholds with a lives cap — validating it against this run. Applies from
    /// the next attempt; typically called once, right after construction.
    ///
    /// # Errors
    /// [`ScoringRulesError`] if the thresholds are zero or not strictly ascending
    /// (see [`ScoringRules::validate`]), or the lives cap is below this run's
    /// starting lives (a contradiction — it would forbid every 1UP).
    pub fn set_scoring(&mut self, scoring: ScoringRules) -> Result<(), ScoringRulesError> {
        scoring.validate()?;
        let starting_lives = self.run.lives().starting();
        if scoring.one_up.max_lives < starting_lives {
            return Err(ScoringRulesError::MaxLivesBelowStart {
                max_lives: scoring.one_up.max_lives,
                starting_lives,
            });
        }
        self.scoring = scoring;
        // No thresholds have fired under the new policy yet. The score is
        // unchanged, but the cursor indexes the new threshold list.
        self.next_threshold = 0;
        Ok(())
    }

    /// Add `points` to the run's score, then grant a 1UP for each not-yet-reached
    /// threshold the new total has crossed — capped at `max_lives`, so a 1UP past
    /// the cap is forfeited rather than banked. Returns how many lives were
    /// granted. The single funnel every award flows through — per-level base,
    /// combo, perfect-clear, and a caller's bonus — so any points can earn a 1UP.
    fn apply_award(&mut self, points: u32) -> u32 {
        self.run.award(points);
        let score = self.run.score().points();
        let mut granted = 0;
        while self.next_threshold < self.scoring.one_up.thresholds.len()
            && score >= self.scoring.one_up.thresholds[self.next_threshold]
        {
            self.next_threshold += 1;
            if self.run.lives().count() < self.scoring.one_up.max_lives {
                self.run.one_up();
                granted += 1;
            }
        }
        granted
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

    /// The current combo length — consecutive successes since the last miss. Zero
    /// at the start of a run and after any miss. A game reads this to render a
    /// live streak meter; the per-attempt streak also rides on [`AttemptOutcome`].
    #[must_use]
    pub fn streak(&self) -> u32 {
        self.streak
    }

    /// The run-long attempt tally — every success and failure this playthrough,
    /// spanning levels. What rank rules judge a finished run by.
    #[must_use]
    pub fn tally(&self) -> RunTally {
        self.tally
    }

    /// The ending title `rules` awards this run as it stands — the first rank
    /// whose requirements the run's own facts (won, run-long failures, points)
    /// meet, or `None` (the game falls back to its plain win / game-over title).
    /// Ranking is a pure read: the run holds no rules, a game passes its own.
    #[must_use]
    pub fn rank<'a>(&self, rules: &'a RankRules) -> Option<&'a str> {
        rules.rank(
            self.phase() == RunPhase::Won,
            self.tally,
            self.run.score().points(),
        )
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
                streak: self.streak,
                streak_bonus: 0,
                perfect_bonus: 0,
                one_ups: 0,
            };
        }

        // The level being played — captured before a clear advances it.
        let current = self.run.levels().current();

        // The run-long tally: rank rules judge the playthrough by it.
        if success {
            self.tally.successes = self.tally.successes.saturating_add(1);
        } else {
            self.tally.failures = self.tally.failures.saturating_add(1);
        }

        // The combo: a success extends the streak, any miss breaks it. The bonus
        // escalates from the second in a row (a miss earns none).
        self.streak = if success {
            self.streak.saturating_add(1)
        } else {
            0
        };
        let base = if success {
            self.levels[current].points_per_success
        } else {
            0
        };
        let streak_bonus = if success {
            self.scoring.streak.bonus(self.streak)
        } else {
            0
        };

        // Judge the level *before* re-arming the goal, so a perfect clear reads
        // this level's own failure count.
        let level_outcome = self.goal.record(success);
        let perfect_bonus = if level_outcome == LevelOutcome::Cleared && self.goal.failures() == 0 {
            self.scoring.perfect_level_points
        } else {
            0
        };

        // Award base + bonuses as one total, then check 1UP thresholds against the
        // new score — so a threshold reached by the combo or perfect bonus counts.
        let total = base
            .saturating_add(streak_bonus)
            .saturating_add(perfect_bonus);
        let one_ups = self.apply_award(total);

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
            streak: self.streak,
            streak_bonus,
            perfect_bonus,
            one_ups,
        }
    }

    /// Award bonus points a caller computes itself — a time bonus for a fast
    /// answer, say. Unlike [`record_attempt`] this does no run sequencing; it adds
    /// the points and checks 1UP thresholds against the new total, so a caller's
    /// bonus can earn a 1UP exactly as the built-in combo and perfect bonuses do.
    /// The caller decides when the bonus is earned (typically on a success,
    /// including the one that wins the run).
    pub fn award(&mut self, points: u32) -> AwardOutcome {
        AwardOutcome {
            one_ups: self.apply_award(points),
        }
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
    /// and its goal re-armed; the combo, 1UP progress, and run-long tally reset
    /// too. The scoring policy and the player profile are left intact.
    pub fn reset(&mut self) {
        self.run.reset();
        self.goal = self.current_goal();
        self.streak = 0;
        self.next_threshold = 0;
        self.tally = RunTally::default();
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
    use super::super::{OneUpRules, RankRule, StreakRules};
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
    fn scoring_defaults_to_a_no_op_so_a_run_scores_only_base_points() {
        // Without `set_scoring`, no combo, perfect, or 1UP fires: the outcome is
        // exactly the pre-scoring one.
        let mut game = GameRun::new(&rules()).unwrap();
        let a = game.record_attempt(true);
        assert_eq!(
            (a.streak, a.streak_bonus, a.perfect_bonus, a.one_ups),
            (1, 0, 0, 0)
        );
        let b = game.record_attempt(true);
        assert_eq!((b.streak_bonus, b.perfect_bonus), (0, 0));
        assert_eq!(game.run().score().points(), 20); // 2 × 10 base, no bonuses
    }

    #[test]
    fn streak_bonus_escalates_spans_levels_and_a_miss_breaks_the_combo() {
        let mut game = GameRun::new(&rules()).unwrap();
        game.set_scoring(ScoringRules {
            streak: StreakRules { bonus_per_step: 5 },
            ..Default::default()
        })
        .unwrap();

        let a = game.record_attempt(true);
        assert_eq!((a.streak, a.streak_bonus), (1, 0)); // first correct: no combo yet
        assert_eq!(game.run().score().points(), 10); // base only

        let b = game.record_attempt(true);
        assert_eq!((b.streak, b.streak_bonus), (2, 5));
        assert_eq!(game.run().score().points(), 25); // +10 base +5 combo

        let c = game.record_attempt(true); // 3rd success clears level 1 (run plays on)
        assert_eq!(c.level_outcome, LevelOutcome::Cleared);
        assert_eq!((c.streak, c.streak_bonus), (3, 10));

        // The combo is run-long, not reset when a level clears.
        let d = game.record_attempt(true);
        assert_eq!((d.streak, d.streak_bonus), (4, 15));

        // Any miss breaks it; the next success opens a fresh combo at one.
        let miss = game.record_attempt(false);
        assert_eq!((miss.streak, miss.streak_bonus), (0, 0));
        let e = game.record_attempt(true);
        assert_eq!((e.streak, e.streak_bonus), (1, 0));
    }

    #[test]
    fn a_perfect_clear_pays_only_when_the_level_was_flawless() {
        let rules = GameRules {
            starting_lives: 3,
            total_levels: 2,
            required_successes: 2,
            max_failures: 1,
            points_per_success: 10,
        };
        let mut game = GameRun::new(&rules).unwrap();
        game.set_scoring(ScoringRules {
            perfect_level_points: 100,
            ..Default::default()
        })
        .unwrap();

        assert_eq!(game.record_attempt(true).perfect_bonus, 0); // 1/2, not cleared yet
        let clean = game.record_attempt(true); // 2/2, zero failures → perfect
        assert_eq!(clean.level_outcome, LevelOutcome::Cleared);
        assert_eq!(clean.perfect_bonus, 100);

        // Level 2: a tolerated failure first, so the clear is not perfect.
        game.record_attempt(false); // failures() == 1 (≤ max), level continues
        game.record_attempt(true); // 1/2
        let blemished = game.record_attempt(true); // 2/2 → cleared, but with a blemish
        assert_eq!(blemished.level_outcome, LevelOutcome::Cleared);
        assert_eq!(blemished.perfect_bonus, 0);
    }

    /// A run long enough to reach 1UP thresholds without clearing or ending: a
    /// single level needing many successes and tolerating many failures.
    fn long_rules() -> GameRules {
        GameRules {
            starting_lives: 2,
            total_levels: 1,
            required_successes: 20,
            max_failures: 20,
            points_per_success: 10,
        }
    }

    #[test]
    fn a_one_up_fires_once_when_the_score_reaches_a_threshold() {
        let mut game = GameRun::new(&long_rules()).unwrap();
        game.set_scoring(ScoringRules {
            one_up: OneUpRules {
                max_lives: 5,
                thresholds: vec![30],
            },
            ..Default::default()
        })
        .unwrap();

        assert_eq!(game.record_attempt(true).one_ups, 0); // 10
        assert_eq!(game.record_attempt(true).one_ups, 0); // 20
        let third = game.record_attempt(true); // 30 → crosses the threshold
        assert_eq!(third.one_ups, 1);
        assert_eq!(game.run().lives().count(), 3); // 2 starting + 1

        // Fires exactly once: further points past it don't re-grant.
        assert_eq!(game.record_attempt(true).one_ups, 0); // 40
        assert_eq!(game.run().lives().count(), 3);
    }

    #[test]
    fn a_bonus_award_can_trigger_a_one_up() {
        // The subtlety the design turns on: a caller awards its own bonus *after*
        // `record_attempt`, so the award path must check thresholds too.
        let mut game = GameRun::new(&long_rules()).unwrap();
        game.set_scoring(ScoringRules {
            one_up: OneUpRules {
                max_lives: 5,
                thresholds: vec![50],
            },
            ..Default::default()
        })
        .unwrap();

        assert_eq!(game.record_attempt(true).one_ups, 0); // 10
        let awarded = game.award(45); // 55 ≥ 50 — a time bonus pushes over the line
        assert_eq!(awarded.one_ups, 1);
        assert_eq!(game.run().lives().count(), 3);
    }

    #[test]
    fn a_single_award_crossing_several_thresholds_grants_several_lives() {
        let mut game = GameRun::new(&long_rules()).unwrap();
        game.set_scoring(ScoringRules {
            one_up: OneUpRules {
                max_lives: 9,
                thresholds: vec![10, 20, 30],
            },
            ..Default::default()
        })
        .unwrap();
        let awarded = game.award(35); // vaults past all three at once
        assert_eq!(awarded.one_ups, 3);
        assert_eq!(game.run().lives().count(), 5); // 2 + 3
    }

    #[test]
    fn a_one_up_past_the_cap_is_forfeited_but_still_consumes_the_threshold() {
        let mut game = GameRun::new(&long_rules()).unwrap(); // starts with 2 lives
        game.set_scoring(ScoringRules {
            one_up: OneUpRules {
                max_lives: 2, // == starting lives: no room to grow
                thresholds: vec![10, 20],
            },
            ..Default::default()
        })
        .unwrap();

        let a = game.record_attempt(true); // 10 → threshold, but already at the cap
        assert_eq!(a.one_ups, 0);
        assert_eq!(game.run().lives().count(), 2);
        // The threshold is still consumed, so it can't linger and fire later.
        let b = game.record_attempt(true); // 20 → the second threshold, still capped
        assert_eq!(b.one_ups, 0);
        assert_eq!(game.run().lives().count(), 2);
    }

    #[test]
    fn set_scoring_rejects_a_cap_below_starting_lives() {
        let mut game = GameRun::new(&rules()).unwrap(); // 2 starting lives
        assert_eq!(
            game.set_scoring(ScoringRules {
                one_up: OneUpRules {
                    max_lives: 1,
                    thresholds: vec![],
                },
                ..Default::default()
            }),
            Err(ScoringRulesError::MaxLivesBelowStart {
                max_lives: 1,
                starting_lives: 2,
            })
        );
    }

    #[test]
    fn set_scoring_rejects_malformed_thresholds() {
        let mut game = GameRun::new(&rules()).unwrap();
        assert_eq!(
            game.set_scoring(ScoringRules {
                one_up: OneUpRules {
                    max_lives: 5,
                    thresholds: vec![0],
                },
                ..Default::default()
            }),
            Err(ScoringRulesError::ZeroThreshold)
        );
        assert_eq!(
            game.set_scoring(ScoringRules {
                one_up: OneUpRules {
                    max_lives: 5,
                    thresholds: vec![20, 10],
                },
                ..Default::default()
            }),
            Err(ScoringRulesError::ThresholdsNotAscending)
        );
    }

    #[test]
    fn reset_clears_the_combo_and_one_up_progress() {
        let mut game = GameRun::new(&long_rules()).unwrap();
        game.set_scoring(ScoringRules {
            streak: StreakRules { bonus_per_step: 5 },
            one_up: OneUpRules {
                max_lives: 9,
                thresholds: vec![20],
            },
            ..Default::default()
        })
        .unwrap();

        game.record_attempt(true); // streak 1, score 10
        game.record_attempt(true); // streak 2, score 25 ≥ 20 → 1UP
        assert_eq!(game.streak(), 2);
        assert_eq!(game.run().lives().count(), 3);

        game.reset();
        assert_eq!(game.streak(), 0);
        assert_eq!(game.run().score().points(), 0);
        assert_eq!(game.run().lives().count(), 2); // refilled to starting

        // The threshold can fire again — its cursor was cleared with the reset.
        game.record_attempt(true); // 10
        let re = game.record_attempt(true); // 25 ≥ 20 again
        assert_eq!(re.one_ups, 1);
    }

    #[test]
    fn the_tally_spans_levels_and_stops_once_the_run_ends() {
        let mut game = GameRun::new(&rules()).unwrap();
        assert_eq!(game.tally(), RunTally::default());

        game.record_attempt(true);
        game.record_attempt(false); // tolerated (max 1), the level continues
        game.record_attempt(true);
        let cleared = game.record_attempt(true); // 3rd success clears level 0
        assert_eq!(cleared.level_outcome, LevelOutcome::Cleared);
        // The tally is run-long: the clear did not reset it like the goal.
        assert_eq!(
            game.tally(),
            RunTally {
                successes: 3,
                failures: 1,
            }
        );

        for _ in 0..3 {
            game.record_attempt(true); // clear level 1 -> the run is won
        }
        assert_eq!(game.phase(), RunPhase::Won);
        let at_win = game.tally();
        game.record_attempt(false); // inert once won: not tallied
        assert_eq!(game.tally(), at_win);

        game.reset();
        assert_eq!(game.tally(), RunTally::default());
    }

    #[test]
    fn rank_reads_the_runs_own_facts() {
        let rank_rules = RankRules {
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
            ],
        };

        // A flawless win earns the proudest rank.
        let mut flawless = GameRun::new(&rules()).unwrap();
        for _ in 0..6 {
            flawless.record_attempt(true);
        }
        assert_eq!(flawless.phase(), RunPhase::Won);
        assert_eq!(flawless.rank(&rank_rules), Some("NO MISS CHAMP"));

        // One tolerated miss on the way drops it to the plain win rank.
        let mut blemished = GameRun::new(&rules()).unwrap();
        blemished.record_attempt(false);
        for _ in 0..6 {
            blemished.record_attempt(true);
        }
        assert_eq!(blemished.phase(), RunPhase::Won);
        assert_eq!(blemished.rank(&rank_rules), Some("MATH MASTER"));

        // A run still playing has not won: no win rank matches it.
        let playing = GameRun::new(&rules()).unwrap();
        assert_eq!(playing.rank(&rank_rules), None);
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

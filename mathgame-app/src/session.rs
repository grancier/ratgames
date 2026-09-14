//! A seeded math run over a compiled campaign.
use mathgame_core::{
    AnswerContract, Evaluation, Generator, GeneratorError, Mix, Problem, Response, Rng, evaluate,
    into_multiple_choice,
};
use ratgames::{
    AnswerMode, AwardOutcome, CampaignError, ContinueRules, GameRun, LevelGoal, LevelOutcome,
    PlayerProfile, RankRules, Run, RunPhase, RunTally, ScoringRules, ScoringRulesError,
};
use std::rc::Rc;

use crate::{MathLevel, format_problem};

mod campaign;
pub use campaign::MathgameCampaign;

#[derive(Debug, thiserror::Error)]
pub enum MathgameSessionError {
    #[error("failed to build a level's problem generator: {0:?}")]
    Generator(GeneratorError),
    #[error("invalid campaign: {0}")]
    Campaign(CampaignError),
    #[error("invalid scoring rules: {0}")]
    Scoring(ScoringRulesError),
}

impl From<GeneratorError> for MathgameSessionError {
    fn from(error: GeneratorError) -> Self {
        Self::Generator(error)
    }
}

impl From<CampaignError> for MathgameSessionError {
    fn from(error: CampaignError) -> Self {
        Self::Campaign(error)
    }
}

impl From<ScoringRulesError> for MathgameSessionError {
    fn from(error: ScoringRulesError) -> Self {
        Self::Scoring(error)
    }
}

#[derive(Debug, Clone)]
pub struct AttemptReport {
    pub correct: bool,
    pub level_outcome: LevelOutcome,
    pub run_phase: RunPhase,
    pub evaluation: Option<Evaluation>,
}

/// A gauntlet level ready to play: its problem mix plus the presentation
/// the session exposes to the screens. Built once from a [`MathLevel`]; the
/// generic goal / reward / input mode live in the run's [`ratgames::Campaign`].
#[derive(Debug)]
struct Level {
    generator: Mix,
    name: String,
    difficulty: String,
}

/// A math-quiz session: the reusable arcade run ([`GameRun`], from ratgames)
/// plus this game's math content — the per-level problem generators, the problem
/// in play, and the last grading. The arcade sequencing (points, lives, levels)
/// lives in [`GameRun`]; this only supplies the current level's math and adapts a
/// graded answer into the `bool` the run records.
#[derive(Debug)]
pub struct MathgameSession {
    game_run: GameRun,
    rng: Rng,
    /// One entry per level, indexed by the run's current level.
    levels: Rc<[Level]>,
    current: Problem,
    last_result: Option<Evaluation>,
}

impl MathgameSession {
    /// Start a session over the `levels` gauntlet (in order), with
    /// `starting_lives` run-wide, seeding the problem sequence with `seed`. Each
    /// level supplies its own math, goal, reward, and input mode; the session
    /// swaps to the next level's generator as the run clears levels.
    ///
    /// # Errors
    /// [`MathgameSessionError`] if a level's generator cannot be built (an empty
    /// or overflowing operand range), or the resulting campaign is not playable
    /// (no levels, zero lives, or a level with an unplayable goal or answer mode).
    pub fn from_levels(
        levels: &[MathLevel],
        starting_lives: u32,
        seed: u64,
    ) -> Result<Self, MathgameSessionError> {
        Ok(MathgameCampaign::from_levels(levels, starting_lives)?.start(seed))
    }

    /// Apply the run's scoring rules — combo, perfect-clear, and 1UP policy — on
    /// top of the base per-level points. A builder step so the caller can thread
    /// its config in fluently: `from_levels(..)?.with_scoring(..)?`. Left off, a
    /// session scores only base points (the reusable no-op default).
    ///
    /// # Errors
    /// [`MathgameSessionError::Scoring`] if the rules are malformed or their lives
    /// cap is below the run's starting lives (see [`GameRun::set_scoring`]).
    pub fn with_scoring(mut self, scoring: ScoringRules) -> Result<Self, MathgameSessionError> {
        self.game_run.set_scoring(scoring)?;
        Ok(self)
    }

    /// Apply the run's continue policy — how many continues a playthrough may use
    /// and whether the score survives one. A builder step like
    /// [`with_scoring`](Self::with_scoring), but infallible (no values of the
    /// policy are degenerate). Left off, a session offers no continues.
    #[must_use]
    pub fn with_continues(mut self, continues: ContinueRules) -> Self {
        self.game_run.set_continues(continues);
        self
    }

    /// Whether the run can continue right now: it is game over and the
    /// playthrough has a continue left to spend.
    #[must_use]
    pub fn can_continue(&self) -> bool {
        self.game_run.can_continue()
    }

    /// Continues left to spend this playthrough.
    #[must_use]
    pub fn continues_remaining(&self) -> u32 {
        self.game_run.continues_remaining()
    }

    /// Consume a continue: resume the game-over run on its current level with
    /// refilled lives (the score per the policy) and a fresh problem. Inert
    /// unless [`can_continue`](Self::can_continue): returns whether the run
    /// actually continued.
    pub fn continue_run(&mut self) -> bool {
        if !self.game_run.continue_run() {
            return false;
        }
        self.last_result = None;
        self.advance_problem();
        true
    }

    #[must_use]
    pub fn profile(&self) -> &PlayerProfile {
        self.game_run.profile()
    }

    pub fn set_player_name(&mut self, name: impl Into<String>) {
        self.game_run.set_player_name(name);
    }

    #[must_use]
    pub fn run(&self) -> Run {
        self.game_run.run()
    }

    #[must_use]
    pub fn goal(&self) -> LevelGoal {
        self.game_run.goal()
    }

    #[must_use]
    pub fn current_problem(&self) -> &Problem {
        &self.current
    }

    /// The current level's display name (e.g. `"NUMBER YARD"`).
    #[must_use]
    pub fn current_level_name(&self) -> &str {
        &self.levels[self.current_level_index()].name
    }

    /// The current level's difficulty label (e.g. `"EASY"`).
    #[must_use]
    pub fn current_difficulty(&self) -> &str {
        &self.levels[self.current_level_index()].difficulty
    }

    #[must_use]
    pub fn current_prompt(&self) -> String {
        format_problem(&self.current)
    }

    #[must_use]
    pub fn current_answer(&self) -> String {
        self.current.canonical_solution().to_fraction_string()
    }

    /// The current problem's answer choices, in display order, when the session
    /// is in multiple-choice mode; `None` for typed answers. The screen renders
    /// these and reports the picked index to [`submit_choice`](Self::submit_choice).
    #[must_use]
    pub fn current_choices(&self) -> Option<Vec<String>> {
        match self.current.answer_contract() {
            AnswerContract::MultipleChoice { options } => {
                Some(options.iter().map(|v| v.to_fraction_string()).collect())
            }
            AnswerContract::FreeForm { .. } => None,
        }
    }

    #[must_use]
    pub fn last_result(&self) -> Option<&Evaluation> {
        self.last_result.as_ref()
    }

    pub fn submit_typed_answer(&mut self, answer: impl Into<String>) -> AttemptReport {
        self.record(Response::Typed(answer.into()))
    }

    /// Grade a picked multiple-choice option by its display index (from
    /// [`current_choices`](Self::current_choices)) and sequence the run.
    pub fn submit_choice(&mut self, index: usize) -> AttemptReport {
        self.record(Response::Selected(index))
    }

    /// The current level's per-question time limit in frames (`0` = untimed) — the
    /// budget a screen arms its question clock with.
    #[must_use]
    pub fn current_time_limit_frames(&self) -> u32 {
        self.game_run.current_level_spec().time_limit_frames
    }

    /// The run-long success/failure tally, spanning levels — what rank rules
    /// judge a finished playthrough by.
    #[must_use]
    pub fn tally(&self) -> RunTally {
        self.game_run.tally()
    }

    /// The ending title `rules` awards the run as it stands, or `None` when no
    /// rank matches (the caller falls back to its plain win / game-over title).
    #[must_use]
    pub fn rank<'a>(&self, rules: &'a RankRules) -> Option<&'a str> {
        self.game_run.rank(rules)
    }

    /// Record the current question as timed out: a miss with no answer. Sequences
    /// the run exactly like a wrong answer (feeds the goal, may cost a life or end
    /// the run) and advances to the next problem if the run continues, but carries
    /// no [`Evaluation`] — the caller shows its own "time up" verdict.
    pub fn time_out(&mut self) -> AttemptReport {
        if self.game_run.phase() != RunPhase::Playing {
            return AttemptReport {
                correct: false,
                level_outcome: self.game_run.goal().outcome(),
                run_phase: self.game_run.phase(),
                evaluation: None,
            };
        }
        let outcome = self.game_run.record_attempt(false);
        self.last_result = None;
        if outcome.run_phase == RunPhase::Playing {
            self.advance_problem();
        }
        AttemptReport {
            correct: false,
            level_outcome: outcome.level_outcome,
            run_phase: outcome.run_phase,
            evaluation: None,
        }
    }

    /// Award bonus points on top of the level reward — e.g. a time bonus for a
    /// fast answer. Delegates to the run controller's [`GameRun::award`]; the
    /// caller computes the amount (this game's product scoring) and awards it on a
    /// success. Returns the [`AwardOutcome`] so a caller can react to a 1UP the
    /// bonus triggered (the score and lives are already updated regardless).
    pub fn award_bonus(&mut self, points: u32) -> AwardOutcome {
        self.game_run.award(points)
    }

    /// Grade `response` against the current problem (math), then let the run
    /// sequence the arcade loop from the bare success/failure. Shared by the
    /// typed and multiple-choice submit paths.
    fn record(&mut self, response: Response) -> AttemptReport {
        if self.game_run.phase() != RunPhase::Playing {
            return AttemptReport {
                correct: false,
                level_outcome: self.game_run.goal().outcome(),
                run_phase: self.game_run.phase(),
                evaluation: None,
            };
        }

        let evaluation = evaluate(&self.current, &response);
        let correct = evaluation.is_correct();
        let outcome = self.game_run.record_attempt(correct);

        self.last_result = Some(evaluation.clone());
        if outcome.run_phase == RunPhase::Playing {
            self.advance_problem();
        }

        AttemptReport {
            correct,
            level_outcome: outcome.level_outcome,
            run_phase: outcome.run_phase,
            evaluation: Some(evaluation),
        }
    }

    /// Restart the run with a clean score, full lives, the first level, and a
    /// fresh problem — reusing the seeded rng so a replay is a new sequence. The
    /// player name is left intact (the result screen returns to the title, which
    /// re-enters it).
    pub fn reset(&mut self) {
        self.game_run.reset();
        self.last_result = None;
        self.advance_problem();
    }

    fn advance_problem(&mut self) {
        let index = self.current_level_index();
        let mode = self.game_run.current_level_spec().answer_mode;
        self.current = make_problem(&self.levels[index].generator, &mut self.rng, mode);
    }

    /// The current level index, clamped into range (the run's index equals the
    /// level count once every level is cleared). `levels` is non-empty by
    /// construction, so this is always valid.
    fn current_level_index(&self) -> usize {
        self.game_run
            .run()
            .levels()
            .current()
            .min(self.levels.len() - 1)
    }
}

/// Generate the next problem in the configured answer mode: a free-form problem
/// for typed answers, or a multiple-choice one (answer plus distractors, drawn
/// from `rng`) otherwise. `options >= 2` is guaranteed at construction, so the
/// conversion never errors; it falls back to the free-form problem rather than
/// panic if it somehow did.
fn make_problem(generator: &Mix, rng: &mut Rng, mode: AnswerMode) -> Problem {
    let base = generator.generate(rng);
    match mode {
        AnswerMode::Typed => base,
        AnswerMode::MultipleChoice { options } => {
            into_multiple_choice(base.clone(), rng, options).unwrap_or(base)
        }
    }
}

#[cfg(test)]
mod tests;

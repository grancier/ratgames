use mathgame_core::{
    AnswerContract, DirectArithmetic, Evaluation, Generator, GeneratorError, Operator, Problem,
    Prompt, Response, Rng, Slot, evaluate, into_multiple_choice,
};
use ratgames::{
    AnswerMode, AnswerModeError, GameRules, GameRulesError, GameRun, LevelGoal, LevelOutcome,
    PlayerProfile, Run, RunPhase,
};

/// Fallback RNG seed for the problem sequence when the wall clock is unavailable.
/// Not a game rule — the arcade rules (lives, levels, goal, points) are
/// [`GameRules`], sourced from config.
pub const STARTER_SEED: u64 = 0x4d41_5448;

#[derive(Debug, thiserror::Error)]
pub enum MathgameSessionError {
    #[error("failed to build the starter addition generator: {0:?}")]
    Generator(GeneratorError),
    #[error("invalid game rules: {0}")]
    Rules(GameRulesError),
    #[error("invalid answer mode: {0}")]
    AnswerMode(AnswerModeError),
}

impl From<GeneratorError> for MathgameSessionError {
    fn from(error: GeneratorError) -> Self {
        Self::Generator(error)
    }
}

impl From<GameRulesError> for MathgameSessionError {
    fn from(error: GameRulesError) -> Self {
        Self::Rules(error)
    }
}

impl From<AnswerModeError> for MathgameSessionError {
    fn from(error: AnswerModeError) -> Self {
        Self::AnswerMode(error)
    }
}

#[derive(Debug, Clone)]
pub struct AttemptReport {
    pub correct: bool,
    pub level_outcome: LevelOutcome,
    pub run_phase: RunPhase,
    pub evaluation: Option<Evaluation>,
}

/// A math-quiz session: the reusable arcade run ([`GameRun`], from ratgames)
/// plus this game's math content — the problem generator, the problem in play,
/// and the last grading. The arcade sequencing (points, lives, levels) lives in
/// [`GameRun`]; this only supplies math and adapts a graded answer into the
/// `bool` the run records.
#[derive(Debug)]
pub struct MathgameSession {
    game_run: GameRun,
    rng: Rng,
    generator: DirectArithmetic,
    answer_mode: AnswerMode,
    current: Problem,
    last_result: Option<Evaluation>,
}

impl MathgameSession {
    /// Start a session under `rules`, answered in `answer_mode`, seeding the
    /// problem sequence with `seed`.
    ///
    /// # Errors
    /// [`MathgameSessionError`] if the starter generator cannot be built, the
    /// `rules` are not playable, or `answer_mode` is multiple choice with fewer
    /// than two options.
    pub fn with_seed(
        rules: &GameRules,
        answer_mode: AnswerMode,
        seed: u64,
    ) -> Result<Self, MathgameSessionError> {
        answer_mode.validate()?;
        let mut rng = Rng::new(seed);
        let generator =
            DirectArithmetic::new("single-digit-addition", "addition", Operator::Add, 0..=9)?;
        let current = make_problem(&generator, &mut rng, answer_mode);
        Ok(Self {
            game_run: GameRun::new(rules)?,
            rng,
            generator,
            answer_mode,
            current,
            last_result: None,
        })
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
        self.current = make_problem(&self.generator, &mut self.rng, self.answer_mode);
    }
}

/// Generate the next problem in the configured answer mode: a free-form problem
/// for typed answers, or a multiple-choice one (answer plus distractors, drawn
/// from `rng`) otherwise. `options >= 2` is guaranteed at construction, so the
/// conversion never errors; it falls back to the free-form problem rather than
/// panic if it somehow did.
fn make_problem(generator: &DirectArithmetic, rng: &mut Rng, mode: AnswerMode) -> Problem {
    let base = generator.generate(rng);
    match mode {
        AnswerMode::Typed => base,
        AnswerMode::MultipleChoice { options } => {
            into_multiple_choice(base.clone(), rng, options).unwrap_or(base)
        }
    }
}

#[must_use]
pub fn format_problem(problem: &Problem) -> String {
    match problem.prompt() {
        Prompt::Equation(equation) => {
            let lhs = equation.lhs().to_fraction_string();
            let rhs = equation.rhs().to_fraction_string();
            let result = equation.result().to_fraction_string();
            let op = operator_symbol(equation.operator());
            match equation.unknown() {
                Slot::Lhs => format!("? {op} {rhs} = {result}"),
                Slot::Rhs => format!("{lhs} {op} ? = {result}"),
                Slot::Result => format!("{lhs} {op} {rhs} = ?"),
            }
        }
    }
}

fn operator_symbol(operator: Operator) -> &'static str {
    match operator {
        Operator::Add => "+",
        Operator::Subtract => "-",
        Operator::Multiply => "x",
        Operator::Divide => "/",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The product rules the app ships (mirrored from defaults.json): the values
    /// these behaviour assertions assume (5 successes clear a level worth 500,
    /// three levels to win, three lives).
    fn rules() -> GameRules {
        GameRules {
            starting_lives: 3,
            total_levels: 3,
            required_successes: 5,
            max_failures: 2,
            points_per_success: 100,
        }
    }

    fn new_session(seed: u64) -> MathgameSession {
        MathgameSession::with_seed(&rules(), AnswerMode::Typed, seed).unwrap()
    }

    fn mc_session(seed: u64, options: usize) -> MathgameSession {
        MathgameSession::with_seed(&rules(), AnswerMode::MultipleChoice { options }, seed).unwrap()
    }

    /// The display index of the correct choice in the current problem.
    fn correct_index(session: &MathgameSession) -> usize {
        let answer = session.current_answer();
        session
            .current_choices()
            .expect("multiple-choice session")
            .iter()
            .position(|choice| *choice == answer)
            .expect("the correct answer is always among the choices")
    }

    fn answer(session: &MathgameSession) -> String {
        session.current_answer()
    }

    #[test]
    fn five_correct_answers_clear_the_first_level_and_award_points() {
        let mut session = new_session(1);

        for _ in 0..4 {
            let report = session.submit_typed_answer(answer(&session));
            assert!(report.correct);
            assert_eq!(report.level_outcome, LevelOutcome::InProgress);
            assert_eq!(report.run_phase, RunPhase::Playing);
        }

        let report = session.submit_typed_answer(answer(&session));

        assert!(report.correct);
        assert_eq!(report.level_outcome, LevelOutcome::Cleared);
        assert_eq!(report.run_phase, RunPhase::Playing);
        assert_eq!(session.run().levels().current(), 1);
        assert_eq!(session.run().score().points(), 500);
        assert_eq!(session.goal().successes(), 0);
        assert_eq!(session.goal().failures(), 0);
    }

    #[test]
    fn exceeding_the_level_failure_limit_costs_one_life_and_resets_the_goal() {
        let mut session = new_session(1);

        for _ in 0..2 {
            let report = session.submit_typed_answer("9999");
            assert!(!report.correct);
            assert_eq!(report.level_outcome, LevelOutcome::InProgress);
            assert_eq!(session.run().lives().count(), 3);
        }

        let report = session.submit_typed_answer("9999");

        assert!(!report.correct);
        assert_eq!(report.level_outcome, LevelOutcome::Failed);
        assert_eq!(report.run_phase, RunPhase::Playing);
        assert_eq!(session.run().lives().count(), 2);
        assert_eq!(session.goal().successes(), 0);
        assert_eq!(session.goal().failures(), 0);
    }

    #[test]
    fn clearing_three_levels_wins_the_run() {
        let mut session = new_session(1);
        let mut last = None;

        for _ in 0..15 {
            last = Some(session.submit_typed_answer(answer(&session)));
        }

        let report = last.unwrap();
        assert_eq!(report.level_outcome, LevelOutcome::Cleared);
        assert_eq!(report.run_phase, RunPhase::Won);
        assert_eq!(session.run().levels().current(), 3);
        assert_eq!(session.run().score().points(), 1500);
    }

    #[test]
    fn three_failed_levels_end_the_run() {
        let mut session = new_session(1);
        let mut last = None;

        for _ in 0..9 {
            last = Some(session.submit_typed_answer("9999"));
        }

        let report = last.unwrap();
        assert_eq!(report.level_outcome, LevelOutcome::Failed);
        assert_eq!(report.run_phase, RunPhase::GameOver);
        assert_eq!(session.run().lives().count(), 0);
    }

    #[test]
    fn reset_restores_a_full_playable_run() {
        let mut session = new_session(1);
        for _ in 0..9 {
            session.submit_typed_answer("9999");
        }
        assert_eq!(session.run().phase(), RunPhase::GameOver);

        session.reset();
        assert_eq!(session.run().phase(), RunPhase::Playing);
        assert_eq!(session.run().lives().count(), 3);
        assert_eq!(session.run().score().points(), 0);
        assert_eq!(session.run().levels().current(), 0);
    }

    #[test]
    fn typed_mode_offers_no_choices() {
        // The default typed session has no multiple-choice options.
        assert!(new_session(1).current_choices().is_none());
    }

    #[test]
    fn multiple_choice_offers_the_configured_options_including_the_answer() {
        let session = mc_session(1, 4);
        let choices = session.current_choices().expect("choices in mc mode");
        assert_eq!(choices.len(), 4);
        assert!(
            choices.contains(&session.current_answer()),
            "the correct answer must be among the choices"
        );
    }

    #[test]
    fn selecting_the_correct_choice_clears_toward_the_level() {
        let mut session = mc_session(1, 4);
        for _ in 0..4 {
            let report = session.submit_choice(correct_index(&session));
            assert!(report.correct);
            assert_eq!(report.level_outcome, LevelOutcome::InProgress);
        }
        let report = session.submit_choice(correct_index(&session));
        assert!(report.correct);
        assert_eq!(report.level_outcome, LevelOutcome::Cleared);
        assert_eq!(session.run().score().points(), 500);
    }

    #[test]
    fn selecting_a_distractor_is_wrong() {
        let mut session = mc_session(1, 4);
        // Any index other than the correct one is a distractor.
        let wrong = (correct_index(&session) + 1) % 4;
        let report = session.submit_choice(wrong);
        assert!(!report.correct);
    }

    #[test]
    fn multiple_choice_needs_at_least_two_options() {
        assert!(matches!(
            MathgameSession::with_seed(&rules(), AnswerMode::MultipleChoice { options: 1 }, 1),
            Err(MathgameSessionError::AnswerMode(
                AnswerModeError::TooFewOptions
            ))
        ));
    }
}

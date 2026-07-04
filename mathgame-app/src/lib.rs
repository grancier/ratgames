use mathgame_core::{
    DirectArithmetic, Evaluation, Generator, GeneratorError, Operator, Problem, Prompt, Response,
    Rng, Slot, evaluate,
};
use ratgames::{
    GameRules, GameRulesError, GameRun, LevelGoal, LevelOutcome, PlayerProfile, Run, RunPhase,
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
    current: Problem,
    last_result: Option<Evaluation>,
}

impl MathgameSession {
    /// Start a session under `rules`, seeding the problem sequence with `seed`.
    ///
    /// # Errors
    /// [`MathgameSessionError`] if the starter generator cannot be built or the
    /// `rules` are not playable.
    pub fn with_seed(rules: &GameRules, seed: u64) -> Result<Self, MathgameSessionError> {
        let mut rng = Rng::new(seed);
        let generator =
            DirectArithmetic::new("single-digit-addition", "addition", Operator::Add, 0..=9)?;
        let current = generator.generate(&mut rng);
        Ok(Self {
            game_run: GameRun::new(rules)?,
            rng,
            generator,
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

    #[must_use]
    pub fn last_result(&self) -> Option<&Evaluation> {
        self.last_result.as_ref()
    }

    pub fn submit_typed_answer(&mut self, answer: impl Into<String>) -> AttemptReport {
        if self.game_run.phase() != RunPhase::Playing {
            return AttemptReport {
                correct: false,
                level_outcome: self.game_run.goal().outcome(),
                run_phase: self.game_run.phase(),
                evaluation: None,
            };
        }

        // Grade the answer (math), then let the run sequence the arcade loop from
        // the bare success/failure.
        let evaluation = evaluate(&self.current, &Response::Typed(answer.into()));
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
        self.current = self.generator.generate(&mut self.rng);
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
        MathgameSession::with_seed(&rules(), seed).unwrap()
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
}

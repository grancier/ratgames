use mathgame_core::{
    DirectArithmetic, Evaluation, Generator, GeneratorError, Operator, Problem, Prompt, Response,
    Rng, Slot, evaluate,
};
use ratgames::{LevelGoal, LevelGoalError, LevelOutcome, PlayerProfile, Run, RunPhase};

pub const STARTING_LIVES: u32 = 3;
pub const TOTAL_LEVELS: usize = 3;
pub const REQUIRED_SUCCESSES: u32 = 5;
pub const MAX_FAILURES: u32 = 2;
pub const POINTS_PER_CORRECT: u32 = 100;
pub const STARTER_SEED: u64 = 0x4d41_5448;

#[derive(Debug, thiserror::Error)]
pub enum MathgameSessionError {
    #[error("failed to build the starter addition generator: {0:?}")]
    Generator(GeneratorError),
    #[error("failed to build the starter level goal: {0:?}")]
    LevelGoal(LevelGoalError),
}

impl From<GeneratorError> for MathgameSessionError {
    fn from(error: GeneratorError) -> Self {
        Self::Generator(error)
    }
}

impl From<LevelGoalError> for MathgameSessionError {
    fn from(error: LevelGoalError) -> Self {
        Self::LevelGoal(error)
    }
}

#[derive(Debug, Clone)]
pub struct AttemptReport {
    pub correct: bool,
    pub level_outcome: LevelOutcome,
    pub run_phase: RunPhase,
    pub evaluation: Option<Evaluation>,
}

#[derive(Debug)]
pub struct MathgameSession {
    profile: PlayerProfile,
    run: Run,
    goal: LevelGoal,
    rng: Rng,
    generator: DirectArithmetic,
    current: Problem,
    last_result: Option<Evaluation>,
}

impl MathgameSession {
    pub fn new() -> Result<Self, MathgameSessionError> {
        Self::with_seed(STARTER_SEED)
    }

    pub fn with_seed(seed: u64) -> Result<Self, MathgameSessionError> {
        let mut rng = Rng::new(seed);
        let generator =
            DirectArithmetic::new("single-digit-addition", "addition", Operator::Add, 0..=9)?;
        let current = generator.generate(&mut rng);
        Ok(Self {
            profile: PlayerProfile::default(),
            run: Run::new(STARTING_LIVES, TOTAL_LEVELS),
            goal: starter_goal()?,
            rng,
            generator,
            current,
            last_result: None,
        })
    }

    #[must_use]
    pub fn profile(&self) -> &PlayerProfile {
        &self.profile
    }

    pub fn set_player_name(&mut self, name: impl Into<String>) {
        self.profile.set_name(name);
    }

    #[must_use]
    pub fn run(&self) -> Run {
        self.run
    }

    #[must_use]
    pub fn goal(&self) -> LevelGoal {
        self.goal
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
        if self.run.phase() != RunPhase::Playing {
            return AttemptReport {
                correct: false,
                level_outcome: self.goal.outcome(),
                run_phase: self.run.phase(),
                evaluation: None,
            };
        }

        let evaluation = evaluate(&self.current, &Response::Typed(answer.into()));
        let correct = evaluation.is_correct();
        if correct {
            self.run.award(POINTS_PER_CORRECT);
        }

        let level_outcome = self.goal.record(correct);
        let run_phase = match level_outcome {
            LevelOutcome::InProgress => self.run.phase(),
            LevelOutcome::Cleared => self.run.clear_level(),
            LevelOutcome::Failed => self.run.fail(),
        };

        self.last_result = Some(evaluation.clone());
        if run_phase == RunPhase::Playing {
            if level_outcome != LevelOutcome::InProgress {
                self.goal =
                    starter_goal().expect("starter level-goal constants are compile-time valid");
            }
            self.advance_problem();
        }

        AttemptReport {
            correct,
            level_outcome,
            run_phase,
            evaluation: Some(evaluation),
        }
    }

    /// Restart the run with a clean score, full lives, the first level, and a
    /// fresh problem — reusing the seeded rng so a replay is a new sequence. The
    /// player name is left intact (the result screen returns to the title, which
    /// re-enters it).
    pub fn reset(&mut self) {
        self.run.reset();
        self.goal = starter_goal().expect("starter level-goal constants are compile-time valid");
        self.last_result = None;
        self.advance_problem();
    }

    fn advance_problem(&mut self) {
        self.current = self.generator.generate(&mut self.rng);
    }
}

fn starter_goal() -> Result<LevelGoal, LevelGoalError> {
    LevelGoal::new(REQUIRED_SUCCESSES, MAX_FAILURES)
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
    use ratgames::{LevelOutcome, RunPhase};

    fn answer(session: &MathgameSession) -> String {
        session.current_answer()
    }

    #[test]
    fn five_correct_answers_clear_the_first_level_and_award_points() {
        let mut session = MathgameSession::with_seed(1).unwrap();

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
        let mut session = MathgameSession::with_seed(1).unwrap();

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
        let mut session = MathgameSession::with_seed(1).unwrap();
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
        let mut session = MathgameSession::with_seed(1).unwrap();
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
        let mut session = MathgameSession::with_seed(1).unwrap();
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

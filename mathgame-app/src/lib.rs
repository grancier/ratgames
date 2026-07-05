use mathgame_core::{
    AnswerContract, DirectArithmetic, Evaluation, Generator, GeneratorError, Operator, Problem,
    Prompt, Response, Rng, Slot, evaluate, into_multiple_choice,
};
use ratgames::{
    AnswerMode, Campaign, CampaignError, GameRun, LevelGoal, LevelOutcome, LevelSpec,
    PlayerProfile, Run, RunPhase,
};

/// Fallback RNG seed for the problem sequence when the wall clock is unavailable.
/// Not a game rule — the arcade rules (lives, per-level goal, reward, and input
/// mode) come from the [`LevelConfig`] gauntlet and the run-wide starting lives,
/// sourced from config.
pub const STARTER_SEED: u64 = 0x4d41_5448;

#[derive(Debug, thiserror::Error)]
pub enum MathgameSessionError {
    #[error("failed to build a level's problem generator: {0:?}")]
    Generator(GeneratorError),
    #[error("invalid campaign: {0}")]
    Campaign(CampaignError),
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

/// A binary arithmetic operator as named in a level file. Maps to the core
/// [`Operator`]; a config-facing enum because `mathgame_core` is dependency-free
/// and so carries no serde of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorConfig {
    Add,
    Subtract,
    Multiply,
    Divide,
}

impl OperatorConfig {
    /// The core operator this names.
    #[must_use]
    pub fn operator(self) -> Operator {
        match self {
            Self::Add => Operator::Add,
            Self::Subtract => Operator::Subtract,
            Self::Multiply => Operator::Multiply,
            Self::Divide => Operator::Divide,
        }
    }
}

/// The coarse skill band the core records on a problem for an operator. The app
/// displays the equation itself, not the band, so this is just sensible metadata.
fn operator_band(operator: Operator) -> &'static str {
    match operator {
        Operator::Add => "addition",
        Operator::Subtract => "subtraction",
        Operator::Multiply => "multiplication",
        Operator::Divide => "division",
    }
}

/// One level of the gauntlet, as authored in a `level_<n>.json` file: the math it
/// drills (an operator over an inclusive operand range), how it presents (a name
/// and difficulty label), and its reusable [`LevelSpec`] rules (win condition,
/// reward, input mode).
///
/// The math is this app's; the rules are a generic `ratgames` type, flattened in
/// so the file stays one flat object — e.g. `{"name":"NUMBER YARD",
/// "operator":"add","min":0,"max":9,"required_successes":5,...}`. Omitted rule
/// fields fall back to [`LevelSpec`] defaults; the math fields are required.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct LevelConfig {
    /// The level's display name (e.g. `"NUMBER YARD"`).
    pub name: String,
    /// A difficulty label shown to the player (e.g. `"EASY"`).
    pub difficulty: String,
    /// The arithmetic operator this level drills.
    pub operator: OperatorConfig,
    /// Inclusive lower bound of the operand range.
    pub min: i64,
    /// Inclusive upper bound of the operand range.
    pub max: i64,
    /// The reusable per-level rules: win condition, reward, and input mode.
    #[serde(flatten)]
    pub rules: LevelSpec,
}

impl LevelConfig {
    /// Build the problem generator this level drills.
    ///
    /// # Errors
    /// [`GeneratorError`] if the operand range is empty or would overflow.
    pub fn generator(&self) -> Result<DirectArithmetic, GeneratorError> {
        let operator = self.operator.operator();
        DirectArithmetic::new(
            self.name.as_str(),
            operator_band(operator),
            operator,
            self.min..=self.max,
        )
    }
}

#[derive(Debug, Clone)]
pub struct AttemptReport {
    pub correct: bool,
    pub level_outcome: LevelOutcome,
    pub run_phase: RunPhase,
    pub evaluation: Option<Evaluation>,
}

/// A gauntlet level ready to play: its problem generator plus the presentation
/// the session exposes to the screens. Built once from a [`LevelConfig`]; the
/// generic goal / reward / input mode live in the run's [`Campaign`].
#[derive(Debug)]
struct Level {
    generator: DirectArithmetic,
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
    levels: Vec<Level>,
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
        levels: &[LevelConfig],
        starting_lives: u32,
        seed: u64,
    ) -> Result<Self, MathgameSessionError> {
        let built = levels
            .iter()
            .map(|config| {
                Ok(Level {
                    generator: config.generator()?,
                    name: config.name.clone(),
                    difficulty: config.difficulty.clone(),
                })
            })
            .collect::<Result<Vec<_>, GeneratorError>>()?;
        let campaign = Campaign {
            starting_lives,
            levels: levels.iter().map(|config| config.rules).collect(),
        };
        // Validates non-emptiness, lives, and every level — so `built[0]` and the
        // current spec below are safe once this succeeds.
        let game_run = GameRun::from_campaign(&campaign)?;
        let mut rng = Rng::new(seed);
        let current = make_problem(
            &built[0].generator,
            &mut rng,
            game_run.current_level_spec().answer_mode,
        );
        Ok(Self {
            game_run,
            rng,
            levels: built,
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
    use ratgames::{AnswerModeError, LevelSpecError};

    /// One single-operator level over 0..=9, worth 100 a success, five to clear,
    /// two misses tolerated — the values these behaviour assertions assume
    /// (mirroring the shipped gauntlet's shape).
    fn level(operator: OperatorConfig, answer_mode: AnswerMode) -> LevelConfig {
        LevelConfig {
            name: "LEVEL".to_string(),
            difficulty: "EASY".to_string(),
            operator,
            min: 0,
            max: 9,
            rules: LevelSpec {
                required_successes: 5,
                max_failures: 2,
                points_per_success: 100,
                answer_mode,
            },
        }
    }

    /// A three-level typed-addition gauntlet — the uniform run the earlier
    /// single-generator session used to hardcode.
    fn typed_levels() -> Vec<LevelConfig> {
        vec![level(OperatorConfig::Add, AnswerMode::Typed); 3]
    }

    fn new_session(seed: u64) -> MathgameSession {
        MathgameSession::from_levels(&typed_levels(), 3, seed).unwrap()
    }

    fn mc_session(seed: u64, options: usize) -> MathgameSession {
        let levels = vec![level(OperatorConfig::Add, AnswerMode::MultipleChoice { options }); 3];
        MathgameSession::from_levels(&levels, 3, seed).unwrap()
    }

    /// Assert the session's current problem uses `operator`.
    fn assert_operator(session: &MathgameSession, operator: Operator) {
        let Prompt::Equation(equation) = session.current_problem().prompt();
        assert_eq!(equation.operator(), operator);
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
        // An unplayable answer mode is caught as the campaign is built, named with
        // its level index.
        assert!(matches!(
            MathgameSession::from_levels(
                &[level(
                    OperatorConfig::Add,
                    AnswerMode::MultipleChoice { options: 1 }
                )],
                3,
                1,
            ),
            Err(MathgameSessionError::Campaign(CampaignError::Level {
                index: 0,
                source: LevelSpecError::AnswerMode(AnswerModeError::TooFewOptions),
            }))
        ));
    }

    #[test]
    fn each_level_drills_its_own_operator() {
        // Clear level 0 (addition), then confirm level 1 poses subtraction — the
        // session swapped generators as the run advanced.
        let levels = vec![
            level(OperatorConfig::Add, AnswerMode::Typed),
            level(OperatorConfig::Subtract, AnswerMode::Typed),
        ];
        let mut session = MathgameSession::from_levels(&levels, 3, 7).unwrap();
        assert_operator(&session, Operator::Add);
        for _ in 0..5 {
            let answer = session.current_answer();
            session.submit_typed_answer(answer);
        }
        assert_eq!(session.run().levels().current(), 1);
        assert_operator(&session, Operator::Subtract);
    }

    #[test]
    fn current_level_name_and_difficulty_track_the_run() {
        let levels = vec![
            LevelConfig {
                name: "NUMBER YARD".to_string(),
                difficulty: "EASY".to_string(),
                ..level(OperatorConfig::Add, AnswerMode::Typed)
            },
            LevelConfig {
                name: "MINUS MINE".to_string(),
                difficulty: "HARD".to_string(),
                ..level(OperatorConfig::Subtract, AnswerMode::Typed)
            },
        ];
        let mut session = MathgameSession::from_levels(&levels, 3, 1).unwrap();
        assert_eq!(session.current_level_name(), "NUMBER YARD");
        assert_eq!(session.current_difficulty(), "EASY");
        for _ in 0..5 {
            let answer = session.current_answer();
            session.submit_typed_answer(answer);
        }
        assert_eq!(session.current_level_name(), "MINUS MINE");
        assert_eq!(session.current_difficulty(), "HARD");
    }

    #[test]
    fn from_levels_rejects_an_empty_gauntlet() {
        assert!(matches!(
            MathgameSession::from_levels(&[], 3, 1),
            Err(MathgameSessionError::Campaign(CampaignError::NoLevels))
        ));
    }

    #[test]
    fn from_levels_rejects_a_bad_operand_range() {
        let bad = LevelConfig {
            min: 5,
            max: 3, // empty range
            ..level(OperatorConfig::Add, AnswerMode::Typed)
        };
        assert!(matches!(
            MathgameSession::from_levels(&[bad], 3, 1),
            Err(MathgameSessionError::Generator(_))
        ));
    }

    #[test]
    fn level_config_parses_a_flat_file_with_defaulted_rules() {
        // Math fields are required; omitted rule fields fall back to LevelSpec
        // defaults, and the operator name maps to the core operator.
        let config: LevelConfig = serde_json::from_str(
            r#"{"name":"NUMBER YARD","difficulty":"EASY","operator":"add","min":0,"max":9,"answer_mode":{"kind":"multiple_choice","options":4}}"#,
        )
        .expect("valid level file");
        assert_eq!(config.name, "NUMBER YARD");
        assert_eq!(config.operator.operator(), Operator::Add);
        assert_eq!(
            config.rules.answer_mode,
            AnswerMode::MultipleChoice { options: 4 }
        );
        // required_successes was omitted, so it takes the LevelSpec default.
        assert_eq!(
            config.rules.required_successes,
            LevelSpec::default().required_successes
        );
        assert!(config.generator().is_ok());
    }
}

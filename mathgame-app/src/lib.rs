use mathgame_core::{
    AnswerContract, DirectArithmetic, Evaluation, Generator, GeneratorError, Operator, Problem,
    Prompt, Response, Rng, Slot, evaluate, into_multiple_choice,
};
use ratgames::{
    AnswerMode, AwardOutcome, Campaign, CampaignError, ContinueRules, GameRun, LevelConfig,
    LevelGoal, LevelOutcome, PlayerProfile, RankRules, Run, RunPhase, RunTally, ScoringRules,
    ScoringRulesError,
};

/// Fallback RNG seed for the problem sequence when the wall clock is unavailable.
/// Not a game rule — the arcade rules (lives, per-level goal, reward, and input
/// mode) come from the [`MathLevel`] gauntlet and the run-wide starting lives,
/// sourced from config.
pub const STARTER_SEED: u64 = 0x4d41_5448;

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

/// The arithmetic a level of the gauntlet drills: an operator over an inclusive
/// operand range. This is `mathgame-app`'s level *content* — the math half of a
/// `level_<n>.json` file, flattened alongside the reusable rules by
/// [`LevelConfig`]. The toolkit stays math-free, so the operator and range live
/// here, not in `ratgames`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct Arithmetic {
    /// The arithmetic operator this level drills.
    pub operator: OperatorConfig,
    /// Inclusive lower bound of the operand range.
    pub min: i64,
    /// Inclusive upper bound of the operand range.
    pub max: i64,
}

impl Arithmetic {
    /// Build the problem generator this level drills, named `name` (the level's
    /// display name, carried on the [`LevelConfig`] shell).
    ///
    /// # Errors
    /// [`GeneratorError`] if the operand range is empty or would overflow.
    pub fn generator(&self, name: &str) -> Result<DirectArithmetic, GeneratorError> {
        let operator = self.operator.operator();
        DirectArithmetic::new(name, operator_band(operator), operator, self.min..=self.max)
    }
}

/// One level of the gauntlet, as authored in a `level_<n>.json` file: the
/// reusable [`LevelConfig`] shell (display name, difficulty label, and the
/// [`LevelSpec`] win-condition/reward/input rules) carrying this app's
/// [`Arithmetic`] content. Both halves are flattened, so the file stays one flat
/// object — e.g. `{"name":"NUMBER YARD","difficulty":"EASY","operator":"add",
/// "min":0,"max":9,"required_successes":5,...}`. Omitted rule fields fall back to
/// [`LevelSpec`] defaults; the name, difficulty, and math fields are required.
pub type MathLevel = LevelConfig<Arithmetic>;

#[derive(Debug, Clone)]
pub struct AttemptReport {
    pub correct: bool,
    pub level_outcome: LevelOutcome,
    pub run_phase: RunPhase,
    pub evaluation: Option<Evaluation>,
}

/// A gauntlet level ready to play: its problem generator plus the presentation
/// the session exposes to the screens. Built once from a [`MathLevel`]; the
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
        levels: &[MathLevel],
        starting_lives: u32,
        seed: u64,
    ) -> Result<Self, MathgameSessionError> {
        let built = levels
            .iter()
            .map(|config| {
                Ok(Level {
                    generator: config.content.generator(&config.name)?,
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
    use ratgames::{AnswerModeError, LevelSpec, LevelSpecError, OneUpRules, RankRule, StreakRules};

    /// One single-operator level over 0..=9, worth 100 a success, five to clear,
    /// two misses tolerated — the values these behaviour assertions assume
    /// (mirroring the shipped gauntlet's shape).
    fn level(operator: OperatorConfig, answer_mode: AnswerMode) -> MathLevel {
        MathLevel {
            name: "LEVEL".to_string(),
            difficulty: "EASY".to_string(),
            rules: LevelSpec {
                required_successes: 5,
                max_failures: 2,
                points_per_success: 100,
                time_limit_frames: 0,
                answer_mode,
            },
            content: Arithmetic {
                operator,
                min: 0,
                max: 9,
            },
        }
    }

    /// A three-level typed-addition gauntlet — the uniform run the earlier
    /// single-generator session used to hardcode.
    fn typed_levels() -> Vec<MathLevel> {
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
    fn with_scoring_applies_the_combo_bonus_and_grants_a_one_up() {
        // A run-wide scoring policy layered over the 100-point base: +10 per combo
        // step, an extra life the first time the score reaches 250.
        let mut session = MathgameSession::from_levels(&typed_levels(), 3, 1)
            .unwrap()
            .with_scoring(ScoringRules {
                streak: StreakRules { bonus_per_step: 10 },
                one_up: OneUpRules {
                    max_lives: 5,
                    thresholds: vec![250],
                },
                ..Default::default()
            })
            .unwrap();

        session.submit_typed_answer(answer(&session)); // 100 (combo 0)
        session.submit_typed_answer(answer(&session)); // +110 → 210 (combo 10)
        session.submit_typed_answer(answer(&session)); // +120 → 330 (combo 20), crosses 250

        assert_eq!(session.run().score().points(), 330); // base 300 + combo 30
        assert_eq!(session.run().lives().count(), 4); // 3 starting + 1UP
    }

    #[test]
    fn with_scoring_rejects_a_lives_cap_below_the_starting_lives() {
        let bad = MathgameSession::from_levels(&typed_levels(), 3, 1)
            .unwrap()
            .with_scoring(ScoringRules {
                one_up: OneUpRules {
                    max_lives: 2, // below the run's 3 starting lives
                    thresholds: vec![],
                },
                ..Default::default()
            });
        assert!(matches!(bad, Err(MathgameSessionError::Scoring(_))));
    }

    #[test]
    fn rank_reflects_the_finished_runs_facts() {
        let rules = RankRules {
            rules: vec![RankRule {
                title: "MATH MASTER".to_string(),
                requires_won: true,
                ..Default::default()
            }],
        };
        let mut session = new_session(1);
        assert_eq!(
            session.rank(&rules),
            None,
            "a run still playing has not won"
        );

        for _ in 0..15 {
            let answer = answer(&session);
            session.submit_typed_answer(answer);
        }
        assert_eq!(session.run().phase(), RunPhase::Won);
        assert_eq!(session.rank(&rules), Some("MATH MASTER"));
        assert_eq!(session.tally().successes, 15);
        assert_eq!(session.tally().failures, 0);
    }

    #[test]
    fn a_continue_resumes_the_run_with_a_fresh_problem() {
        let mut session = MathgameSession::from_levels(&typed_levels(), 3, 1)
            .unwrap()
            .with_continues(ContinueRules {
                allowed: 1,
                keep_score: true,
            });
        assert!(!session.can_continue(), "nothing to continue while playing");

        while session.run().phase() == RunPhase::Playing {
            session.submit_typed_answer("9999");
        }
        assert_eq!(session.run().phase(), RunPhase::GameOver);
        assert!(session.can_continue());
        assert_eq!(session.continues_remaining(), 1);

        assert!(session.continue_run());
        assert_eq!(session.run().phase(), RunPhase::Playing);
        assert_eq!(session.run().lives().count(), 3);
        assert!(session.last_result().is_none(), "the old grading is gone");
        // The resumed run poses a live problem: answering it counts.
        let answer = answer(&session);
        assert!(session.submit_typed_answer(answer).correct);
        assert_eq!(session.continues_remaining(), 0);
    }

    #[test]
    fn timeouts_are_misses_that_can_fail_a_level_and_cost_a_life() {
        // A timeout sequences the run exactly like a wrong answer, but grades no
        // answer — the two-miss tolerance then fails the level on the third.
        let mut session = new_session(1);
        for _ in 0..2 {
            let report = session.time_out();
            assert!(!report.correct);
            assert!(report.evaluation.is_none(), "a timeout grades no answer");
            assert_eq!(report.level_outcome, LevelOutcome::InProgress);
            assert_eq!(session.run().lives().count(), 3);
        }
        let failed = session.time_out();
        assert_eq!(failed.level_outcome, LevelOutcome::Failed);
        assert_eq!(session.run().lives().count(), 2);
    }

    #[test]
    fn award_bonus_adds_points_and_time_limit_reads_the_level() {
        // The bundled typed gauntlet is untimed; a bonus still adds to the score.
        let mut session = new_session(1);
        assert_eq!(session.current_time_limit_frames(), 0);
        session.award_bonus(70);
        assert_eq!(session.run().score().points(), 70);

        // A level that sets a time limit reports its per-question budget.
        let timed = MathLevel {
            rules: LevelSpec {
                time_limit_frames: 480,
                ..LevelSpec::default()
            },
            ..level(OperatorConfig::Add, AnswerMode::Typed)
        };
        let timed = MathgameSession::from_levels(&[timed], 3, 1).unwrap();
        assert_eq!(timed.current_time_limit_frames(), 480);
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
            MathLevel {
                name: "NUMBER YARD".to_string(),
                difficulty: "EASY".to_string(),
                ..level(OperatorConfig::Add, AnswerMode::Typed)
            },
            MathLevel {
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
        let bad = MathLevel {
            content: Arithmetic {
                min: 5,
                max: 3, // empty range
                operator: OperatorConfig::Add,
            },
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
        let config: MathLevel = serde_json::from_str(
            r#"{"name":"NUMBER YARD","difficulty":"EASY","operator":"add","min":0,"max":9,"answer_mode":{"kind":"multiple_choice","options":4}}"#,
        )
        .expect("valid level file");
        assert_eq!(config.name, "NUMBER YARD");
        assert_eq!(config.content.operator.operator(), Operator::Add);
        assert_eq!(
            config.rules.answer_mode,
            AnswerMode::MultipleChoice { options: 4 }
        );
        // required_successes was omitted, so it takes the LevelSpec default.
        assert_eq!(
            config.rules.required_successes,
            LevelSpec::default().required_successes
        );
        assert!(config.content.generator(&config.name).is_ok());
    }
}

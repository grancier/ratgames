use ratgames::{
    AnswerMode, AwardOutcome, Campaign, CampaignError, ContinueRules, GameRun, LevelConfig,
    LevelGoal, LevelOutcome, PlayerProfile, RankRules, Run, RunPhase, RunTally, ScoringRules,
    ScoringRulesError,
};
use wordgame_core::{GeneratorError, Puzzle, PuzzleGenerator, Rng, WordList};

/// Fallback RNG seed for the puzzle sequence when the wall clock is
/// unavailable. Not a game rule — the arcade rules (lives, per-level goal,
/// reward, and input mode) come from the [`WordLevel`] gauntlet and the
/// run-wide starting lives, sourced from config.
pub const STARTER_SEED: u64 = 0x574f_5244;

#[derive(Debug, thiserror::Error)]
pub enum WordgameSessionError {
    #[error("failed to build a level's puzzle generator: {0}")]
    Generator(#[from] GeneratorError),
    #[error("invalid campaign: {0}")]
    Campaign(#[from] CampaignError),
    #[error("invalid scoring rules: {0}")]
    Scoring(#[from] ScoringRulesError),
    /// The missing-letter game answers with the shared input field, one letter
    /// at a time — a level asking for multiple choice is a config mistake.
    #[error("level {level:?} asks for multiple choice; wordgame answers are typed")]
    TypedAnswersOnly { level: String },
}

/// The words a level of the gauntlet poses: every pool word whose length falls
/// in the inclusive window, each hiding `blanks` letters. This is
/// `wordgame-app`'s level *content* — the spelling half of a `level_<n>.json`
/// file, flattened alongside the reusable rules by [`LevelConfig`]. The toolkit
/// stays word-free, so the length window and blank count live here, not in
/// `ratgames`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct Words {
    /// Inclusive shortest word length this level poses.
    pub length_min: usize,
    /// Inclusive longest word length this level poses.
    pub length_max: usize,
    /// Hidden letters per word, filled left to right by the typed answer.
    pub blanks: usize,
}

impl Words {
    /// Build this level's puzzle generator over `pool`.
    ///
    /// # Errors
    /// [`GeneratorError`] if the length window is empty or matches no pool
    /// word, no letter would be hidden, or the blank count would blot out the
    /// shortest eligible word.
    pub fn generator(&self, pool: &WordList) -> Result<PuzzleGenerator, GeneratorError> {
        PuzzleGenerator::new(pool, self.length_min..=self.length_max, self.blanks)
    }
}

/// One level of the gauntlet, as authored in a `level_<n>.json` file: the
/// reusable [`LevelConfig`] shell (display name, difficulty label, and the
/// `LevelSpec` win-condition/reward/input rules) carrying this app's [`Words`]
/// content. Both halves are flattened, so the file stays one flat object —
/// e.g. `{"name":"WORD POND","difficulty":"EASY","length_min":3,
/// "length_max":4,"blanks":1,"required_successes":5,...}`.
pub type WordLevel = LevelConfig<Words>;

/// The result of one graded attempt (an answer or a timeout): whether it hit,
/// how it sequenced the level and the run, and — on a graded miss — the full
/// word the player failed to spell, for the verdict to reveal.
#[derive(Debug, Clone)]
pub struct AttemptReport {
    pub correct: bool,
    pub level_outcome: LevelOutcome,
    pub run_phase: RunPhase,
    /// The full solution word on a graded miss; `None` on a hit or a timeout
    /// (which grades no answer).
    pub revealed: Option<String>,
}

/// A gauntlet level ready to play: its puzzle shape plus the presentation the
/// session exposes to the screens. Built once from a [`WordLevel`]; the
/// generic goal / reward / input rules live in the run's [`Campaign`].
#[derive(Debug)]
struct Level {
    generator: PuzzleGenerator,
    name: String,
    difficulty: String,
}

/// A missing-letter session: the reusable arcade run ([`GameRun`], from
/// ratgames) plus this game's spelling content — the per-level puzzle
/// generators and the puzzle in play. The arcade sequencing (points, lives,
/// levels) lives in [`GameRun`]; this only supplies the current level's word
/// and adapts a graded answer into the `bool` the run records.
#[derive(Debug)]
pub struct WordgameSession {
    game_run: GameRun,
    rng: Rng,
    /// One entry per level, indexed by the run's current level.
    levels: Vec<Level>,
    current: Puzzle,
}

impl WordgameSession {
    /// Start a session over the `levels` gauntlet (in order), posing words
    /// from `pool`, with `starting_lives` run-wide, seeding the puzzle
    /// sequence with `seed`. Each level supplies its own shape, goal, reward,
    /// and clock; the session swaps to the next level's generator as the run
    /// clears levels.
    ///
    /// # Errors
    /// [`WordgameSessionError`] if a level's generator cannot be built (an
    /// empty window, no eligible words, a blank count that blots out a word),
    /// a level asks for multiple choice (answers are typed), or the resulting
    /// campaign is not playable (no levels, zero lives, an unplayable goal).
    pub fn from_levels(
        levels: &[WordLevel],
        pool: &WordList,
        starting_lives: u32,
        seed: u64,
    ) -> Result<Self, WordgameSessionError> {
        if let Some(config) = levels
            .iter()
            .find(|config| matches!(config.rules.answer_mode, AnswerMode::MultipleChoice { .. }))
        {
            return Err(WordgameSessionError::TypedAnswersOnly {
                level: config.name.clone(),
            });
        }
        let built = levels
            .iter()
            .map(|config| {
                Ok(Level {
                    generator: config.content.generator(pool)?,
                    name: config.name.clone(),
                    difficulty: config.difficulty.clone(),
                })
            })
            .collect::<Result<Vec<_>, GeneratorError>>()?;
        let campaign = Campaign {
            starting_lives,
            levels: levels.iter().map(|config| config.rules).collect(),
        };
        // Validates non-emptiness, lives, and every level — so `built[0]` is
        // safe once this succeeds.
        let game_run = GameRun::from_campaign(&campaign)?;
        let mut rng = Rng::new(seed);
        let current = built[0].generator.generate(&mut rng);
        Ok(Self {
            game_run,
            rng,
            levels: built,
            current,
        })
    }

    /// Apply the run's scoring rules — combo, perfect-clear, and 1UP policy —
    /// on top of the base per-level points. A builder step so the caller can
    /// thread its config in fluently: `from_levels(..)?.with_scoring(..)?`.
    /// Left off, a session scores only base points (the reusable no-op
    /// default).
    ///
    /// # Errors
    /// [`WordgameSessionError::Scoring`] if the rules are malformed or their
    /// lives cap is below the run's starting lives (see [`GameRun::set_scoring`]).
    pub fn with_scoring(mut self, scoring: ScoringRules) -> Result<Self, WordgameSessionError> {
        self.game_run.set_scoring(scoring)?;
        Ok(self)
    }

    /// Apply the run's continue policy — how many continues a playthrough may
    /// use and whether the score survives one. Infallible, like the policy.
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
    /// refilled lives (the score per the policy) and a fresh puzzle. Inert
    /// unless [`can_continue`](Self::can_continue): returns whether the run
    /// actually continued.
    pub fn continue_run(&mut self) -> bool {
        if !self.game_run.continue_run() {
            return false;
        }
        self.advance_puzzle();
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
    pub fn current_puzzle(&self) -> &Puzzle {
        &self.current
    }

    /// The current level's display name (e.g. `"WORD POND"`).
    #[must_use]
    pub fn current_level_name(&self) -> &str {
        &self.levels[self.current_level_index()].name
    }

    /// The current level's difficulty label (e.g. `"EASY"`).
    #[must_use]
    pub fn current_difficulty(&self) -> &str {
        &self.levels[self.current_level_index()].difficulty
    }

    /// The prompt banner: the word with `_` at each hidden letter — `"C_T"`.
    #[must_use]
    pub fn current_prompt(&self) -> String {
        self.current.masked()
    }

    /// The letters that clear the current puzzle, in reading order — what the
    /// player types into the input field.
    #[must_use]
    pub fn current_answer(&self) -> String {
        self.current.missing_letters()
    }

    /// The current puzzle's full solution word, UPPERCASE.
    #[must_use]
    pub fn current_word(&self) -> &str {
        self.current.word()
    }

    /// Grade the typed letters against the current puzzle and sequence the
    /// run. A miss carries the full word for the verdict to reveal.
    pub fn submit_typed_answer(&mut self, answer: impl Into<String>) -> AttemptReport {
        if self.game_run.phase() != RunPhase::Playing {
            return AttemptReport {
                correct: false,
                level_outcome: self.game_run.goal().outcome(),
                run_phase: self.game_run.phase(),
                revealed: None,
            };
        }
        let answer = answer.into();
        let correct = self.current.grade(&answer);
        // Capture the reveal before advancing deals the next puzzle.
        let revealed = (!correct).then(|| self.current.word().to_string());
        let outcome = self.game_run.record_attempt(correct);
        if outcome.run_phase == RunPhase::Playing {
            self.advance_puzzle();
        }
        AttemptReport {
            correct,
            level_outcome: outcome.level_outcome,
            run_phase: outcome.run_phase,
            revealed,
        }
    }

    /// The current level's per-question time limit in frames (`0` = untimed) —
    /// the budget a screen arms its question clock with.
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

    /// Record the current question as timed out: a miss with no answer.
    /// Sequences the run exactly like a wrong answer (feeds the goal, may cost
    /// a life or end the run) and advances to the next puzzle if the run
    /// continues, but reveals nothing — the caller shows its own "time up"
    /// verdict.
    pub fn time_out(&mut self) -> AttemptReport {
        if self.game_run.phase() != RunPhase::Playing {
            return AttemptReport {
                correct: false,
                level_outcome: self.game_run.goal().outcome(),
                run_phase: self.game_run.phase(),
                revealed: None,
            };
        }
        let outcome = self.game_run.record_attempt(false);
        if outcome.run_phase == RunPhase::Playing {
            self.advance_puzzle();
        }
        AttemptReport {
            correct: false,
            level_outcome: outcome.level_outcome,
            run_phase: outcome.run_phase,
            revealed: None,
        }
    }

    /// Award bonus points on top of the level reward — e.g. a time bonus for a
    /// fast answer. Delegates to the run controller's [`GameRun::award`]; the
    /// caller computes the amount (this game's product scoring) and awards it
    /// on a success. Returns the [`AwardOutcome`] so a caller can react to a
    /// 1UP the bonus triggered (the score and lives are already updated
    /// regardless).
    pub fn award_bonus(&mut self, points: u32) -> AwardOutcome {
        self.game_run.award(points)
    }

    /// Restart the run with a clean score, full lives, the first level, and a
    /// fresh puzzle — reusing the seeded rng so a replay is a new sequence.
    /// The player name is left intact (the result screen returns to the title,
    /// which re-enters it).
    pub fn reset(&mut self) {
        self.game_run.reset();
        self.advance_puzzle();
    }

    fn advance_puzzle(&mut self) {
        let index = self.current_level_index();
        self.current = self.levels[index].generator.generate(&mut self.rng);
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratgames::{LevelSpec, OneUpRules, RankRule, StreakRules};

    /// A pool with words at every ladder length, so shape tests can pick
    /// exactly the lengths they mean.
    fn pool() -> WordList {
        WordList::new([
            "CAT", "DOG", "SUN", "BUS", "HAT", "BIRD", "FISH", "FROG", "STAR", "APPLE", "CRANE",
            "BREAD", "BANANA", "CHERRY", "PUMPKIN", "PENGUIN",
        ])
        .unwrap()
    }

    /// A one-shape spec: words in `min..=max` hiding `blanks` letters.
    fn words(min: usize, max: usize, blanks: usize) -> Words {
        Words {
            length_min: min,
            length_max: max,
            blanks,
        }
    }

    /// One typed level of the given shape, worth 100 a success, five to clear,
    /// two misses tolerated — mirroring the shipped gauntlet's shape.
    fn level(content: Words) -> WordLevel {
        WordLevel {
            name: "LEVEL".to_string(),
            difficulty: "EASY".to_string(),
            rules: LevelSpec {
                required_successes: 5,
                max_failures: 2,
                points_per_success: 100,
                time_limit_frames: 0,
                answer_mode: AnswerMode::Typed,
            },
            content,
        }
    }

    /// A three-level single-blank gauntlet over the 3-letter words.
    fn typed_levels() -> Vec<WordLevel> {
        vec![level(words(3, 3, 1)); 3]
    }

    fn new_session(seed: u64) -> WordgameSession {
        WordgameSession::from_levels(&typed_levels(), &pool(), 3, seed).unwrap()
    }

    fn answer(session: &WordgameSession) -> String {
        session.current_answer()
    }

    #[test]
    fn the_prompt_masks_the_word_and_the_answer_fills_the_blanks() {
        let session = new_session(1);
        let prompt = session.current_prompt();
        let word = session.current_word();
        assert_eq!(prompt.chars().count(), word.chars().count());
        assert_eq!(prompt.chars().filter(|&c| c == '_').count(), 1);
        assert_eq!(session.current_answer().chars().count(), 1);
        // Filling the blank with the answer reconstructs the word.
        let filled: String = prompt
            .chars()
            .zip(word.chars())
            .map(|(masked, real)| if masked == '_' { real } else { masked })
            .collect();
        assert_eq!(filled, word);
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
    fn a_correct_answer_is_accepted_lowercase() {
        let mut session = new_session(3);
        let report = session.submit_typed_answer(answer(&session).to_ascii_lowercase());
        assert!(report.correct);
        assert!(report.revealed.is_none(), "a hit reveals nothing");
    }

    #[test]
    fn a_wrong_answer_reveals_the_full_word() {
        let mut session = new_session(1);
        let word = session.current_word().to_string();
        let report = session.submit_typed_answer("0");
        assert!(!report.correct);
        assert_eq!(
            report.revealed.as_deref(),
            Some(word.as_str()),
            "the verdict reveals the word that was posed, not the next one"
        );
    }

    #[test]
    fn with_scoring_applies_the_combo_bonus_and_grants_a_one_up() {
        // A run-wide scoring policy layered over the 100-point base: +10 per
        // combo step, an extra life the first time the score reaches 250.
        let mut session = WordgameSession::from_levels(&typed_levels(), &pool(), 3, 1)
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
        let bad = WordgameSession::from_levels(&typed_levels(), &pool(), 3, 1)
            .unwrap()
            .with_scoring(ScoringRules {
                one_up: OneUpRules {
                    max_lives: 2, // below the run's 3 starting lives
                    thresholds: vec![],
                },
                ..Default::default()
            });
        assert!(matches!(bad, Err(WordgameSessionError::Scoring(_))));
    }

    #[test]
    fn rank_reflects_the_finished_runs_facts() {
        let rules = RankRules {
            rules: vec![RankRule {
                title: "WORD WIZARD".to_string(),
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
        assert_eq!(session.rank(&rules), Some("WORD WIZARD"));
        assert_eq!(session.tally().successes, 15);
        assert_eq!(session.tally().failures, 0);
    }

    #[test]
    fn a_continue_resumes_the_run_with_a_fresh_puzzle() {
        let mut session = WordgameSession::from_levels(&typed_levels(), &pool(), 3, 1)
            .unwrap()
            .with_continues(ContinueRules {
                allowed: 1,
                keep_score: true,
            });
        assert!(!session.can_continue(), "nothing to continue while playing");

        while session.run().phase() == RunPhase::Playing {
            session.submit_typed_answer("0");
        }
        assert_eq!(session.run().phase(), RunPhase::GameOver);
        assert!(session.can_continue());
        assert_eq!(session.continues_remaining(), 1);

        assert!(session.continue_run());
        assert_eq!(session.run().phase(), RunPhase::Playing);
        assert_eq!(session.run().lives().count(), 3);
        // The resumed run poses a live puzzle: answering it counts.
        let answer = answer(&session);
        assert!(session.submit_typed_answer(answer).correct);
        assert_eq!(session.continues_remaining(), 0);
    }

    #[test]
    fn timeouts_are_misses_that_can_fail_a_level_and_cost_a_life() {
        // A timeout sequences the run exactly like a wrong answer, but grades
        // no answer — the two-miss tolerance then fails the level on the third.
        let mut session = new_session(1);
        for _ in 0..2 {
            let report = session.time_out();
            assert!(!report.correct);
            assert!(report.revealed.is_none(), "a timeout reveals nothing");
            assert_eq!(report.level_outcome, LevelOutcome::InProgress);
            assert_eq!(session.run().lives().count(), 3);
        }
        let failed = session.time_out();
        assert_eq!(failed.level_outcome, LevelOutcome::Failed);
        assert_eq!(session.run().lives().count(), 2);
    }

    #[test]
    fn award_bonus_adds_points_and_time_limit_reads_the_level() {
        // The test gauntlet is untimed; a bonus still adds to the score.
        let mut session = new_session(1);
        assert_eq!(session.current_time_limit_frames(), 0);
        session.award_bonus(70);
        assert_eq!(session.run().score().points(), 70);

        // A level that sets a time limit reports its per-question budget.
        let timed = WordLevel {
            rules: LevelSpec {
                time_limit_frames: 480,
                required_successes: 5,
                max_failures: 2,
                points_per_success: 100,
                answer_mode: AnswerMode::Typed,
            },
            ..level(words(3, 3, 1))
        };
        let timed = WordgameSession::from_levels(&[timed], &pool(), 3, 1).unwrap();
        assert_eq!(timed.current_time_limit_frames(), 480);
    }

    #[test]
    fn exceeding_the_level_failure_limit_costs_one_life_and_resets_the_goal() {
        let mut session = new_session(1);

        for _ in 0..2 {
            let report = session.submit_typed_answer("0");
            assert!(!report.correct);
            assert_eq!(report.level_outcome, LevelOutcome::InProgress);
            assert_eq!(session.run().lives().count(), 3);
        }

        let report = session.submit_typed_answer("0");

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
            last = Some(session.submit_typed_answer("0"));
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
            session.submit_typed_answer("0");
        }
        assert_eq!(session.run().phase(), RunPhase::GameOver);

        session.reset();
        assert_eq!(session.run().phase(), RunPhase::Playing);
        assert_eq!(session.run().lives().count(), 3);
        assert_eq!(session.run().score().points(), 0);
        assert_eq!(session.run().levels().current(), 0);
    }

    #[test]
    fn each_level_poses_its_own_shape() {
        // Clear level 0 (3-letter, one blank), then confirm level 1 poses
        // 5-letter words with two blanks — the session swapped generators as
        // the run advanced.
        let levels = vec![level(words(3, 3, 1)), level(words(5, 5, 2))];
        let mut session = WordgameSession::from_levels(&levels, &pool(), 3, 7).unwrap();
        assert_eq!(session.current_word().len(), 3);
        for _ in 0..5 {
            let answer = session.current_answer();
            session.submit_typed_answer(answer);
        }
        assert_eq!(session.run().levels().current(), 1);
        assert_eq!(session.current_word().len(), 5);
        assert_eq!(session.current_puzzle().blank_count(), 2);
    }

    #[test]
    fn current_level_name_and_difficulty_track_the_run() {
        let levels = vec![
            WordLevel {
                name: "WORD POND".to_string(),
                difficulty: "EASY".to_string(),
                ..level(words(3, 3, 1))
            },
            WordLevel {
                name: "THE WORD SUMMIT".to_string(),
                difficulty: "HARD".to_string(),
                ..level(words(5, 5, 2))
            },
        ];
        let mut session = WordgameSession::from_levels(&levels, &pool(), 3, 1).unwrap();
        assert_eq!(session.current_level_name(), "WORD POND");
        assert_eq!(session.current_difficulty(), "EASY");
        for _ in 0..5 {
            let answer = session.current_answer();
            session.submit_typed_answer(answer);
        }
        assert_eq!(session.current_level_name(), "THE WORD SUMMIT");
        assert_eq!(session.current_difficulty(), "HARD");
    }

    #[test]
    fn from_levels_rejects_an_empty_gauntlet() {
        assert!(matches!(
            WordgameSession::from_levels(&[], &pool(), 3, 1),
            Err(WordgameSessionError::Campaign(CampaignError::NoLevels))
        ));
    }

    #[test]
    fn from_levels_rejects_a_multiple_choice_level() {
        let mut mc = level(words(3, 3, 1));
        mc.name = "PICKY".to_string();
        mc.rules.answer_mode = AnswerMode::MultipleChoice { options: 4 };
        assert!(matches!(
            WordgameSession::from_levels(&[mc], &pool(), 3, 1),
            Err(WordgameSessionError::TypedAnswersOnly { level }) if level == "PICKY"
        ));
    }

    #[test]
    fn from_levels_rejects_an_unbuildable_shape() {
        // No pool word is eight letters long.
        assert!(matches!(
            WordgameSession::from_levels(&[level(words(8, 9, 1))], &pool(), 3, 1),
            Err(WordgameSessionError::Generator(
                GeneratorError::NoWordsInRange { min: 8, max: 9 }
            ))
        ));
        // Three blanks would blot out a three-letter word.
        assert!(matches!(
            WordgameSession::from_levels(&[level(words(3, 4, 3))], &pool(), 3, 1),
            Err(WordgameSessionError::Generator(
                GeneratorError::TooManyBlanks { .. }
            ))
        ));
        // A puzzle must hide at least one letter.
        assert!(matches!(
            WordgameSession::from_levels(&[level(words(3, 4, 0))], &pool(), 3, 1),
            Err(WordgameSessionError::Generator(GeneratorError::NoBlanks))
        ));
    }

    #[test]
    fn level_config_parses_a_flat_file_with_defaulted_rules() {
        // The shape fields are required; omitted rule fields fall back to
        // LevelSpec defaults.
        let config: WordLevel = serde_json::from_str(
            r#"{"name":"WORD POND","difficulty":"EASY","length_min":3,"length_max":4,"blanks":1}"#,
        )
        .expect("valid level file");
        assert_eq!(config.name, "WORD POND");
        assert_eq!(
            config.content,
            Words {
                length_min: 3,
                length_max: 4,
                blanks: 1
            }
        );
        assert_eq!(config.rules.answer_mode, AnswerMode::Typed);
        assert_eq!(
            config.rules.required_successes,
            LevelSpec::default().required_successes
        );
        assert!(config.content.generator(&pool()).is_ok());
    }
}

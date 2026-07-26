//! The example-local quiz domain — the moved counterpart of the retired
//! `ratgames::quiz` module, adapted to sit over a [`GameRun`].
//!
//! `ratgames` owns the reusable arcade sequencing ([`GameRun`]: lives,
//! levels, score, win / game-over). This module owns the game-specific part a real
//! consumer writes: the questions, grading a typed answer, and advancing to the
//! next question. It is pure — no rendering, no windowing, no frame timing (the
//! presentation beat lives in `main`) — and the seam into the run is a bare
//! `bool`, so no quiz detail crosses into the toolkit.

use ratgames::{GameRules, GameRulesError, GameRun, Run, RunPhase};

/// A question and the answer that satisfies it.
///
/// Ported verbatim from the retired `ratgames::Question`: the reusable grader is
/// whitespace-tolerant and numeric when both sides parse (so `"012"` matches
/// `"12"`), otherwise it compares trimmed text.
#[derive(Debug, Clone)]
pub struct Question {
    prompt: String,
    expected: String,
}

impl Question {
    #[must_use]
    pub fn new(prompt: impl Into<String>, expected: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            expected: expected.into(),
        }
    }

    #[must_use]
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// Whether `answer` satisfies the question.
    #[must_use]
    pub fn accepts(&self, answer: &str) -> bool {
        let a = answer.trim();
        let e = self.expected.trim();
        match (a.parse::<i64>(), e.parse::<i64>()) {
            (Ok(x), Ok(y)) => x == y,
            _ => a == e,
        }
    }
}

/// The result of grading one answer: whether it was correct, and where the run
/// stands after recording it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Graded {
    pub correct: bool,
    pub run_phase: RunPhase,
}

/// A tiny arithmetic quiz driving a [`GameRun`]. It cycles a fixed question bank;
/// each correct answer records a success (scoring points, clearing levels), each
/// wrong one a failure (costing lives) — the run decides win and game over.
#[derive(Debug)]
pub struct Quiz {
    run: GameRun,
    questions: Vec<Question>,
    index: usize,
}

impl Quiz {
    /// Start a quiz under `rules` over a non-empty `questions` bank.
    ///
    /// # Errors
    /// Returns [`GameRulesError`] if `rules` are not playable.
    ///
    /// # Panics
    /// Panics if `questions` is empty — the example always supplies a bank, so a
    /// caller error should fail loudly rather than defer to an index panic later.
    pub fn new(rules: &GameRules, questions: Vec<Question>) -> Result<Self, GameRulesError> {
        assert!(!questions.is_empty(), "a quiz needs at least one question");
        Ok(Self {
            run: GameRun::new(rules)?,
            questions,
            index: 0,
        })
    }

    /// The prompt for the current question.
    #[must_use]
    pub fn prompt(&self) -> &str {
        self.questions[self.index].prompt()
    }

    /// The arcade run behind the quiz — score, lives, phase — for a HUD readout.
    #[must_use]
    pub fn run(&self) -> Run {
        self.run.run()
    }

    /// Grade `answer`, record the attempt to the run, and — while the run keeps
    /// playing — advance to the next question. Returns the verdict and the run's
    /// new phase.
    pub fn answer(&mut self, answer: &str) -> Graded {
        let correct = self.questions[self.index].accepts(answer);
        let outcome = self.run.record_attempt(correct);
        if outcome.run_phase == RunPhase::Playing {
            self.index = (self.index + 1) % self.questions.len();
        }
        Graded {
            correct,
            run_phase: outcome.run_phase,
        }
    }

    /// Restart for a fresh playthrough from the first question.
    pub fn reset(&mut self) {
        self.run.reset();
        self.index = 0;
    }
}

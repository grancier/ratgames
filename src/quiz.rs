//! The math-quiz state machine: one question, a retry loop, and a win.
//!
//! Pure and frame-driven, mirroring [`Transition`](crate::scene::Transition):
//! the presenter pumps [`Quiz::submit`] on Enter and [`Quiz::advance`] every
//! frame, then reads [`Quiz::phase`] and [`Quiz::cross_visible`] to pick which
//! layers to draw. There is no rendering and no I/O here — only the rules.
//!
//! This is the seam the wider math game grows from. Menus, a named player, a
//! running score, and per-name levels all sit *around* this: a session would own
//! the score and feed the quiz a fresh [`Question`] per level instead of the one
//! fixed question wired today.

use crate::config::{FlashConfig, QuizConfig};

/// The result of grading a submitted answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Correct,
    Wrong,
}

/// The externally visible phase — what the presenter should show. Projected from
/// the internal, frame-counting state so callers never see the counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Waiting for input; the question shows in the field.
    Asking,
    /// A wrong answer is being rejected; the cross is blinking.
    Rejecting,
    /// The game-over sign is lingering before the retry.
    GameOver,
    /// The answer was correct; the win banner plays. Terminal.
    Won,
}

/// A question and the answer that satisfies it.
#[derive(Debug, Clone)]
pub struct Question {
    prompt: String,
    expected: String,
}

impl Question {
    /// A question whose `prompt` is shown to the player and whose `expected`
    /// answer wins.
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

    /// Whether `answer` satisfies the question. Surrounding whitespace is
    /// ignored; if both the answer and the expected value parse as integers they
    /// are compared numerically (so `"012"` matches `"12"`), otherwise they are
    /// compared as trimmed text.
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

/// Internal state: [`Phase`] plus the frame counters that drive the blink and
/// the game-over linger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Asking,
    Rejecting { frame: u32 },
    GameOver { frame: u32 },
    Won,
}

/// The quiz rules: grade answers, blink on a miss, linger on game-over, then loop
/// back to the question; settle on a win.
#[derive(Debug, Clone)]
pub struct Quiz {
    question: Question,
    flash: FlashConfig,
    game_over_frames: u32,
    state: State,
}

impl Quiz {
    /// Build from parts: the `question`, the cross `flash` timing, and how many
    /// frames the game-over sign lingers before the retry.
    #[must_use]
    pub fn new(question: Question, flash: FlashConfig, game_over_frames: u32) -> Self {
        Self {
            question,
            flash,
            game_over_frames,
            state: State::Asking,
        }
    }

    /// Build straight from a [`QuizConfig`].
    #[must_use]
    pub fn from_config(config: &QuizConfig) -> Self {
        Self::new(
            Question::new(config.question.clone(), config.expected.clone()),
            config.flash,
            config.game_over_frames,
        )
    }

    /// The prompt to show in the input field.
    #[must_use]
    pub fn prompt(&self) -> &str {
        self.question.prompt()
    }

    /// The current presentation phase.
    #[must_use]
    pub fn phase(&self) -> Phase {
        match self.state {
            State::Asking => Phase::Asking,
            State::Rejecting { .. } => Phase::Rejecting,
            State::GameOver { .. } => Phase::GameOver,
            State::Won => Phase::Won,
        }
    }

    /// Whether the quiz is accepting input.
    #[must_use]
    pub fn is_asking(&self) -> bool {
        matches!(self.state, State::Asking)
    }

    /// Whether the reject cross is visible this frame (only while rejecting).
    #[must_use]
    pub fn cross_visible(&self) -> bool {
        match self.state {
            State::Rejecting { frame } => self.flash.visible_at(frame),
            _ => false,
        }
    }

    /// Grade `answer`. Only acts while [`asking`](Self::is_asking): a correct
    /// answer settles on [`Phase::Won`], a wrong one starts the blink. Returns
    /// `None` if not currently asking.
    pub fn submit(&mut self, answer: &str) -> Option<Outcome> {
        if !self.is_asking() {
            return None;
        }
        if self.question.accepts(answer) {
            self.state = State::Won;
            Some(Outcome::Correct)
        } else {
            self.state = State::Rejecting { frame: 0 };
            Some(Outcome::Wrong)
        }
    }

    /// Advance one frame, driving the blink then the game-over linger and finally
    /// looping back to the question. A no-op while asking or won.
    pub fn advance(&mut self) {
        self.state = match self.state {
            State::Rejecting { frame } => {
                let next = frame + 1;
                if next >= self.flash.total_frames() {
                    State::GameOver { frame: 0 }
                } else {
                    State::Rejecting { frame: next }
                }
            }
            State::GameOver { frame } => {
                let next = frame + 1;
                if next >= self.game_over_frames {
                    State::Asking
                } else {
                    State::GameOver { frame: next }
                }
            }
            other => other,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiz() -> Quiz {
        Quiz::from_config(&QuizConfig::default())
    }

    #[test]
    fn accepts_is_numeric_and_whitespace_tolerant() {
        let q = Question::new("What is 6+6? ", "12");
        assert!(q.accepts("12"));
        assert!(q.accepts("  12 "));
        assert!(q.accepts("012"));
        assert!(!q.accepts("13"));
        assert!(!q.accepts(""));
        assert!(!q.accepts("twelve"));
    }

    #[test]
    fn correct_answer_wins_and_is_terminal() {
        let mut q = quiz();
        assert_eq!(q.phase(), Phase::Asking);
        assert_eq!(q.submit("12"), Some(Outcome::Correct));
        assert_eq!(q.phase(), Phase::Won);
        q.advance();
        assert_eq!(q.phase(), Phase::Won); // stays won
        assert_eq!(q.submit("12"), None); // no further input accepted
    }

    #[test]
    fn wrong_answer_flashes_then_games_over_then_retries() {
        let cfg = QuizConfig::default();
        let mut q = Quiz::from_config(&cfg);

        assert_eq!(q.submit("7"), Some(Outcome::Wrong));
        assert_eq!(q.phase(), Phase::Rejecting);
        assert!(q.cross_visible()); // visible on the first frame

        for _ in 0..cfg.flash.total_frames() {
            q.advance();
        }
        assert_eq!(q.phase(), Phase::GameOver);
        assert!(!q.cross_visible());

        for _ in 0..cfg.game_over_frames {
            q.advance();
        }
        assert_eq!(q.phase(), Phase::Asking); // looped back for another try
        assert!(q.is_asking());
    }

    #[test]
    fn submit_is_ignored_outside_asking() {
        let mut q = quiz();
        q.submit("7"); // -> Rejecting
        assert_eq!(q.submit("12"), None); // ignored mid-animation
        assert_eq!(q.phase(), Phase::Rejecting);
    }

    #[test]
    fn cross_blinks_the_configured_number_of_times() {
        let cfg = QuizConfig::default();
        let mut q = Quiz::from_config(&cfg);
        q.submit("7");

        // Count rising edges of visibility across the whole sequence.
        let mut edges = 0;
        let mut prev = false;
        for _ in 0..cfg.flash.total_frames() {
            let now = q.cross_visible();
            if now && !prev {
                edges += 1;
            }
            prev = now;
            q.advance();
        }
        assert_eq!(edges, cfg.flash.count);
    }
}

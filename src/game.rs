//! [`MathGame`] — the session that drives the math quiz.
//!
//! It owns the [`Quiz`] rules and every layer (the reject cross, the game-over
//! sign, the win marquee, the input field), and maps the current phase to the
//! layers drawn this frame. This is the *composition policy* that otherwise
//! accretes in a window loop; keeping it here leaves the binary/example as a
//! thin window + event pump, and gives the roadmap's menus / player / score a
//! place to grow.
//!
//! It is deliberately windowing-agnostic — no `minifb`. Input arrives as method
//! calls ([`type_char`](Self::type_char) / [`backspace`](Self::backspace) /
//! [`submit`](Self::submit)), a frame advances with [`tick`](Self::tick), and a
//! frame is composed with [`render`](Self::render) into a caller-owned surface.

use crate::config::{Config, ConfigError};
use crate::font::SystemFont;
use crate::input::InputField;
use crate::marquee::Marquee;
use crate::placard::Placard;
use crate::present::{OverlayLayer, PixelLayer, Presentation};
use crate::quiz::{Phase, Quiz};
use crate::surface::Surface;

/// The math quiz as a self-contained, frame-driven session. Every layer and
/// colour comes from [`Config`]; the only thing supplied at runtime is a loaded
/// [`SystemFont`] for the input overlay.
#[derive(Debug)]
pub struct MathGame {
    quiz: Quiz,
    cross: Placard,
    game_over: Placard,
    win: Marquee,
    input: InputField,
}

impl MathGame {
    /// Build every layer from `config`, using `font` for the input overlay.
    ///
    /// # Errors
    /// Returns [`ConfigError`] if a banner's raster glyph source font cannot load
    /// or a banner would exceed the sprite size limits.
    pub fn new(config: &Config, font: SystemFont) -> Result<Self, ConfigError> {
        let cross = Placard::new(config.quiz.cross.sprite()?);
        let game_over = Placard::new(config.quiz.game_over.sprite()?);
        let win = Marquee::new(
            config.marquee.text_sprite(&config.quiz.win_text)?,
            config.marquee.speed,
        );

        let quiz = Quiz::from_config(&config.quiz);
        let input = InputField::new(config.input.clone(), font).with_prompt(quiz.prompt());

        Ok(Self {
            quiz,
            cross,
            game_over,
            win,
            input,
        })
    }

    /// Type a character into the answer. Ignored unless the quiz is asking, so
    /// keystrokes during an animation are dropped rather than buffered.
    pub fn type_char(&mut self, ch: char) {
        if self.quiz.is_asking() {
            self.input.type_char(ch);
        }
    }

    /// Delete the character before the caret. Ignored unless asking.
    pub fn backspace(&mut self) {
        if self.quiz.is_asking() {
            self.input.backspace();
        }
    }

    /// Grade the current answer and start the win or reject sequence. A no-op
    /// unless asking.
    pub fn submit(&mut self) {
        if self.quiz.is_asking() {
            let answer = self.input.submit();
            self.quiz.submit(&answer);
        }
    }

    /// Advance one frame: step the quiz, and scroll the win banner once won.
    pub fn tick(&mut self) {
        self.quiz.advance();
        if self.quiz.phase() == Phase::Won {
            self.win.advance();
        }
    }

    /// The current presentation phase.
    #[must_use]
    pub fn phase(&self) -> Phase {
        self.quiz.phase()
    }

    /// Whether the quiz is accepting input this frame.
    #[must_use]
    pub fn is_asking(&self) -> bool {
        self.quiz.is_asking()
    }

    /// The answer typed so far (empty between rounds).
    #[must_use]
    pub fn answer(&self) -> &str {
        self.input.line().text()
    }

    /// Compose this frame into `framebuffer` via `presentation`, choosing which
    /// pixel-art layer to show by phase and keeping the input field up until the
    /// win screen.
    pub fn render(&self, presentation: &mut Presentation, framebuffer: &mut Surface) {
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        match self.quiz.phase() {
            Phase::Asking => {}
            Phase::Rejecting => {
                if self.quiz.cross_visible() {
                    world.push(&self.cross);
                }
            }
            Phase::GameOver => world.push(&self.game_over),
            Phase::Won => world.push(&self.win),
        }

        let overlays: Vec<&dyn OverlayLayer> = if self.quiz.phase() == Phase::Won {
            Vec::new()
        } else {
            vec![&self.input]
        };

        presentation.render(&world, &overlays, framebuffer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> MathGame {
        let config = Config::default();
        let font = SystemFont::load(&config.input.font).expect("a system font");
        MathGame::new(&config, font).expect("default config builds")
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn types_an_answer_and_wins() {
        let mut g = game();
        assert!(g.is_asking());
        for ch in "12".chars() {
            g.type_char(ch);
        }
        assert_eq!(g.answer(), "12");
        g.submit();
        assert_eq!(g.phase(), Phase::Won);
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn wrong_answer_runs_the_reject_then_retries() {
        let config = Config::default();
        let font = SystemFont::load(&config.input.font).expect("a system font");
        let mut g = MathGame::new(&config, font).expect("default config builds");

        g.type_char('7');
        g.submit();
        assert_eq!(g.phase(), Phase::Rejecting);
        for _ in 0..config.quiz.flash.total_frames() {
            g.tick();
        }
        assert_eq!(g.phase(), Phase::GameOver);
        for _ in 0..config.quiz.game_over_frames {
            g.tick();
        }
        assert_eq!(g.phase(), Phase::Asking);
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn input_is_ignored_outside_asking() {
        let mut g = game();
        g.type_char('7');
        g.submit(); // -> Rejecting
        assert_eq!(g.phase(), Phase::Rejecting);
        g.type_char('9'); // gated: not asking
        assert_eq!(g.answer(), ""); // submit cleared it; nothing new buffered
    }

    #[test]
    #[ignore = "requires a system font; run with `cargo test -- --ignored`"]
    fn render_shows_the_win_banner_after_a_correct_answer() {
        let config = Config::default();
        let font = SystemFont::load(&config.input.font).expect("a system font");
        let mut g = MathGame::new(&config, font).expect("default config builds");
        for ch in config.quiz.expected.chars() {
            g.type_char(ch);
        }
        g.submit();
        g.tick();
        assert_eq!(g.phase(), Phase::Won);

        let s = config.screen;
        let mut pres = Presentation::new(s.size, s.backdrop, s.letterbox, s.min_scale);
        let mut fb = Surface::new(s.size, s.backdrop);
        g.render(&mut pres, &mut fb);
        let green = config.marquee.colors.fill;
        assert!(
            fb.as_slice().iter().any(|&w| w == green.packed()),
            "the win marquee should paint its green fill"
        );
    }
}

//! `math_game` — a worked example wiring the ratgames toolkit into a tiny quiz.
//!
//! This is the canonical *consumer* shape. A small demo-local quiz ([`quiz`])
//! supplies the game's content — questions and grading — and drives a reusable
//! [`ratgames::GameRun`] for the arcade sequencing (lives, levels, score, win /
//! game over). The play loop itself is the library's [`ChallengeScreen`]: the
//! demo implements its [`Challenge`] driver — bake the view for the current
//! question, grade an answer into a [`FeedbackBeat`], route the resolution —
//! and the screen owns the phase machinery (input frozen under the beat, the
//! reject blink and verdict hold, fire-once resolution). The terminal cards
//! are a [`PromptScreen`] (GAME OVER) and a tiny marquee screen (the scrolling
//! YOU WIN); Enter restarts from either. Nothing math-specific lives in the
//! library.
//!
//! Unlike the simpler demos this one shares durable state across screens, so
//! it carries its own rich [`Ctx`] (the quiz, the one input field, the glyph
//! source) rather than the crate's [`DemoCtx`](crate::DemoCtx). Build one with
//! [`context`], stack [`challenge_screen`] on it, and drive it on any host.

mod banner;
mod quiz;

use ratgames::{
    BannerAnchor, BannerContext, Blink, Challenge, ChallengeAnswer, ChallengeResolution,
    ChallengeScreen, ChallengeView, Color, Config, Countdown, FeedbackBeat, GameRules,
    GradedAttempt, InputContext, InputField, InputLine, Marquee, OverlayLayer, PixelLayer, Point,
    PromptExit, PromptScreen, RasterGlyphSource, RunPhase, Screen, ScreenChange, ShadowBanner,
    ShadowBannerFactory, ShadowStyle, Size, SystemFont, TextColors, UiInput, palette,
};

use crate::DemoError;
use banner::Banner;
use quiz::{Question, Quiz};

/// The virtual screen the demo's preset composes into — the same 640×360 the
/// fixed-size demos use, so the input field renders at exactly the device
/// scale `text_input`'s does.
pub const VIRTUAL: Size = Size { w: 640, h: 360 };
/// Source-pixel height of the raster glyph source the banners bake through —
/// the 64px banner standard the games share. `scale` stays small because the
/// resolution already lives in the source (`scale` ≠ resolution).
const BANNER_CELL_PX: u32 = 64;
/// Source-pixel height of the HUD's own smaller glyph source — the 32px
/// body-text standard: the lives/score line is body text, and at the banner
/// size it would span the whole screen.
const HUD_CELL_PX: u32 = 32;

/// The demo's own preset over the neutral library defaults: just the shared
/// virtual screen (the glyph sizes above already follow the product standard).
/// A `--config` file still overrides it.
#[must_use]
pub fn default_config() -> Config {
    let mut config = Config::default();
    config.screen.size = VIRTUAL;
    config
}
/// Frames the feedback verdict holds before the next question (or the terminal
/// card) appears.
const VERDICT_HOLD_FRAMES: u32 = 30;
/// The success wash: a translucent green tint fading out over the verdict hold.
const CORRECT_WASH: Color = Color::argb(0x55, 0x39, 0xD3, 0x53);

/// The demo's arcade rules: three lives, two levels, three correct answers to
/// clear a level, a third miss on a level fails it, 100 points a success. A real
/// game reads these from config; the demo fixes them in Rust.
fn rules() -> GameRules {
    GameRules {
        starting_lives: 3,
        total_levels: 2,
        required_successes: 3,
        max_failures: 2,
        points_per_success: 100,
    }
}

/// The demo's fixed question bank, cycled as the run advances.
fn questions() -> Vec<Question> {
    [
        ("6 + 6 = ", "12"),
        ("7 + 5 = ", "12"),
        ("9 + 4 = ", "13"),
        ("8 + 7 = ", "15"),
        ("4 + 9 = ", "13"),
        ("5 + 8 = ", "13"),
    ]
    .into_iter()
    .map(|(prompt, answer)| Question::new(prompt, answer))
    .collect()
}

/// The durable session state every screen shares: the quiz, the one input
/// field, the glyph source banners bake through, and the quit flag the host
/// loop watches.
pub struct Ctx {
    /// Set when the player asks to leave (Esc); the host loop stops on it.
    pub quit: bool,
    quiz: Quiz,
    input: InputField,
    glyphs: RasterGlyphSource,
    /// The smaller face body text (the HUD readout) bakes through.
    hud_glyphs: RasterGlyphSource,
    virtual_size: Size,
    /// The win marquee's scroll speed and palette, from the config.
    marquee_speed: u32,
    marquee_colors: TextColors,
}

/// The shared context, its fonts and styling resolved from `config` (the
/// input font doubles as the banner glyph face; `SystemFont` isn't `Clone`, so
/// the face loads twice — cheap).
///
/// # Errors
/// [`DemoError::Font`] if the font cannot be loaded; [`DemoError::Rules`] if
/// the fixed rules were degenerate.
pub fn context(config: &Config) -> Result<Ctx, DemoError> {
    let input_font = SystemFont::load(&config.input.font)?;
    let glyphs = RasterGlyphSource::new(SystemFont::load(&config.input.font)?, BANNER_CELL_PX);
    let hud_glyphs = RasterGlyphSource::new(SystemFont::load(&config.input.font)?, HUD_CELL_PX);
    Ok(Ctx {
        quit: false,
        quiz: Quiz::new(&rules(), questions())?,
        input: InputField::new(config.input.clone(), input_font),
        glyphs,
        hud_glyphs,
        virtual_size: config.screen.size,
        marquee_speed: config.marquee.speed,
        marquee_colors: config.marquee.colors,
    })
}

/// Generic pixel-art screens composite through the demo's banner style; body
/// text (the HUD line) bakes through the smaller face.
impl BannerContext for Ctx {
    fn banner_factory(&self) -> ShadowBannerFactory<'_> {
        ShadowBannerFactory::new(&self.glyphs, ShadowStyle::default(), self.virtual_size)
    }

    fn hud_factory(&self) -> ShadowBannerFactory<'_> {
        ShadowBannerFactory::new(&self.hud_glyphs, ShadowStyle::default(), self.virtual_size)
    }
}

/// The challenge screen edits the one durable field through the text-entry seam.
impl InputContext for Ctx {
    fn input_line(&mut self) -> &mut InputLine {
        self.input.line_mut()
    }

    fn input_overlay(&self) -> &dyn OverlayLayer {
        &self.input
    }
}

/// The lives/score readout, anchored top-left, in the HUD's body-text face.
fn status_line(ctx: &Ctx) -> ShadowBanner {
    let run = ctx.quiz.run();
    ctx.hud_factory().at(
        &format!(
            "LIVES {}  SCORE {}",
            run.lives().count(),
            run.score().points()
        ),
        Point::new(8, 8),
        1,
    )
}

/// Where a resolved feedback beat routes: the next question, or a terminal card.
enum Pending {
    Next,
    GameOver,
    Won,
}

/// The game half of the [`ChallengeScreen`]: content from the [`Quiz`], grading
/// into a [`FeedbackBeat`], and the terminal routing. Stateless — the quiz and
/// the widgets live in the shared [`Ctx`].
struct MathChallenge;

impl MathChallenge {
    /// The graded shape for one attempt: a miss opens with the flashing reject
    /// cross, a hit tints the screen with a fading wash; both hold the verdict.
    fn graded(ctx: &Ctx, correct: bool, phase: RunPhase) -> GradedAttempt<Pending> {
        let (reject, wash, verdict) = if correct {
            (None, Some(CORRECT_WASH), "CORRECT!")
        } else {
            (Some(reject_cross(ctx)), None, "WRONG")
        };
        GradedAttempt {
            beat: FeedbackBeat::new(
                reject,
                wash,
                ctx.banner_factory().centered(verdict, 1),
                Countdown::new(VERDICT_HOLD_FRAMES),
            ),
            status: status_line(ctx),
            pending: match phase {
                RunPhase::Playing => Pending::Next,
                RunPhase::GameOver => Pending::GameOver,
                RunPhase::Won => Pending::Won,
            },
        }
    }
}

impl Challenge<Ctx> for MathChallenge {
    type Pending = Pending;

    fn view(&mut self, ctx: &Ctx) -> ChallengeView<Ctx> {
        // The equation is the big centred banner; the answer types into the
        // shared field; no choice list, no question clock.
        ChallengeView {
            prompt: ctx.banner_factory().centered(ctx.quiz.prompt(), 1),
            status: status_line(ctx),
            choices: None,
            gauge: None,
        }
    }

    fn grade(
        &mut self,
        answer: ChallengeAnswer,
        _time_left: Option<u32>,
        ctx: &mut Ctx,
    ) -> GradedAttempt<Pending> {
        // Typed answers only: the view never offers choices, so grade a stray
        // pick as an empty answer rather than panic.
        let text = match answer {
            ChallengeAnswer::Typed(text) => text,
            ChallengeAnswer::Choice(_) => String::new(),
        };
        let graded = ctx.quiz.answer(&text);
        Self::graded(ctx, graded.correct, graded.run_phase)
    }

    fn time_out(&mut self, ctx: &mut Ctx) -> GradedAttempt<Pending> {
        // No question clock is ever armed here; grade the impossible timeout as
        // an empty miss so the machinery stays total.
        let graded = ctx.quiz.answer("");
        Self::graded(ctx, graded.correct, graded.run_phase)
    }

    fn resolve(&mut self, pending: Pending, ctx: &mut Ctx) -> ChallengeResolution<Ctx> {
        match pending {
            Pending::Next => ChallengeResolution::Stay,
            Pending::GameOver => {
                ChallengeResolution::Leave(ScreenChange::Replace(game_over_screen(ctx)))
            }
            Pending::Won => ChallengeResolution::Leave(ScreenChange::Replace(win_screen(ctx))),
        }
    }

    fn cancel(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        ctx.quit = true;
        ScreenChange::None
    }
}

/// The flat red reject cross, blinked centre-screen on a miss.
fn reject_cross(ctx: &Ctx) -> Blink {
    let cross = Banner {
        text: "X".to_string(),
        scale: 2,
        tracking: 0,
        shadow_depth: 0, // a flat cross: red fill + outline, no 3D
        outline_px: 1,
        gap: 0,
        colors: TextColors {
            fill: palette::DANGER,
            outline: palette::OUTLINE,
            shadow: palette::OUTLINE, // unused at depth 0
        },
    }
    .sprite(&ctx.glyphs);
    Blink::new(cross, BannerAnchor::Center, ctx.virtual_size)
        .scale(1)
        .pattern(3, 8, 8)
}

/// A fresh play screen over the shared quiz, its first view baked from the
/// current state.
#[must_use]
pub fn challenge_screen(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    Box::new(ChallengeScreen::new(MathChallenge, ctx))
}

/// Reset the run and deal a fresh play screen — Enter on either terminal card.
fn restart(ctx: &mut Ctx) -> ScreenChange<Ctx> {
    ctx.quiz.reset();
    ScreenChange::Replace(challenge_screen(ctx))
}

/// The GAME OVER card: a [`PromptScreen`] holding the banner until Enter
/// restarts or Esc quits.
fn game_over_screen(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    let banner = ctx.banner_factory().centered("GAME OVER", 1);
    Box::new(PromptScreen::new(
        vec![banner],
        |exit, ctx: &mut Ctx| match exit {
            PromptExit::Confirmed => restart(ctx),
            PromptExit::Cancelled => {
                ctx.quit = true;
                ScreenChange::None
            }
            PromptExit::Idled => ScreenChange::None,
        },
    ))
}

/// The win screen: the scrolling YOU WIN marquee — a ticking [`PixelLayer`], so
/// a static [`PromptScreen`] cannot host it. Enter restarts, Esc quits.
struct WinScreen {
    marquee: Marquee,
}

impl Screen<Ctx> for WinScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Confirm => restart(ctx),
            UiInput::Cancel => {
                ctx.quit = true;
                ScreenChange::None
            }
            _ => ScreenChange::None,
        }
    }

    fn tick(&mut self, _ctx: &mut Ctx) -> ScreenChange<Ctx> {
        self.marquee.advance();
        ScreenChange::None
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        world: &mut Vec<&'a dyn PixelLayer>,
        _overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        world.push(&self.marquee);
    }
}

fn win_screen(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    let sprite = Banner {
        text: "YOU WIN".to_string(),
        scale: 2,
        tracking: 1,
        shadow_depth: 3,
        outline_px: 1,
        gap: 6,
        colors: ctx.marquee_colors,
    }
    .sprite(&ctx.glyphs);
    Box::new(WinScreen {
        marquee: Marquee::new(sprite, ctx.marquee_speed),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratgames::ScreenStack;

    /// The demo preset with every font swapped for the crate-bundled face —
    /// the same construction the web build performs, deterministic everywhere.
    fn embedded_config() -> Config {
        let mut config = default_config();
        config.input.font = config.input.font.with_embedded_font();
        config
    }

    #[test]
    fn the_preset_composes_on_the_shared_screen() {
        assert_eq!(default_config().screen.size, VIRTUAL);
    }

    #[test]
    fn a_full_round_grades_through_the_challenge_screen() {
        let mut ctx = context(&embedded_config()).expect("embedded fonts load");
        let mut stack = ScreenStack::new(challenge_screen(&ctx));

        // Type the first question's answer (6 + 6 = 12) and submit.
        for ch in "12".chars() {
            stack.handle(UiInput::Char(ch), &mut ctx);
        }
        stack.handle(UiInput::Confirm, &mut ctx);
        // The feedback beat runs; skipping it with Confirm resolves to the
        // next question and the run has banked the points.
        stack.handle(UiInput::Confirm, &mut ctx);
        assert_eq!(ctx.quiz.run().score().points(), 100, "one success banked");
        assert!(!ctx.quit);

        // Esc routes out through the driver's cancel.
        stack.handle(UiInput::Cancel, &mut ctx);
        assert!(ctx.quit);
    }
}

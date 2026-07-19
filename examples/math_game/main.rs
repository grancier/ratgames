//! `math_game` — a worked example wiring the ratgames toolkit into a tiny quiz.
//!
//! This is the canonical *consumer* shape. A small example-local quiz
//! ([`quiz`]) supplies the game's content — questions and grading — and drives a
//! reusable [`ratgames::GameRun`] for the arcade sequencing (lives, levels,
//! score, win / game over). The presentation is composed from generic ratgames
//! layers — the anti-aliased [`InputField`], a flashing [`Blink`] reject cross,
//! a [`Placard`] GAME OVER sign, a scrolling [`Marquee`] win banner — and the
//! whole thing runs on the toolkit's own window host: a [`QuizScreen`] on a
//! [`ScreenStack`], driven by [`MinifbHost::run`], so this example writes no
//! window loop of its own. Nothing math-specific lives in the library.
//!
//! Run with `cargo run --example math_game --features minifb`; type an answer,
//! Enter submits, Backspace edits, Esc (or close) quits. From the win / game-over
//! screen, Enter restarts. Pass `--config <file>` to load a TOML/JSON `Config`
//! for the window / screen / input styling.

mod banner;
mod quiz;

use anyhow::Result;
use ratgames::{
    BannerAnchor, Blink, ConfigSource, GameRules, InputField, Marquee, MinifbHost, OverlayLayer,
    PixelLayer, Placard, Presentation, RasterGlyphSource, RunPhase, Screen, ScreenChange,
    ScreenStack, Size, Sprite, SystemFont, TextColors, UiInput, palette, parse_config_flag,
};

use banner::Banner;
use quiz::{Graded, Question, Quiz};

/// Source-pixel height of the raster glyph source the banners bake through — a
/// crisper look than the chunky 8x8 bitmap. `scale` stays small because the
/// resolution already lives in the source (`scale` ≠ resolution).
const BANNER_CELL_PX: u32 = 32;

/// The demo's arcade rules: three lives, two levels, three correct answers to
/// clear a level, a third miss on a level fails it, 100 points a success. A real
/// game reads these from config; the example fixes them in Rust.
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

/// Which layers show this frame. The [`Quiz`] decides the run's phase; this
/// sequences the on-screen feedback around it.
enum Beat {
    /// Waiting for input: the answer field is live.
    Asking,
    /// A miss just landed: flash the red cross over the frozen field.
    Rejecting(Blink),
    /// The run ended in a loss: the GAME OVER sign.
    GameOver,
    /// The run ended in a win: the scrolling marquee.
    Won,
}

/// The beat a graded answer moves to: a win / game-over banner when the run
/// ends, a flashing cross on a miss that keeps playing, else straight back to
/// asking the next question.
fn next_beat(graded: Graded, cross: &Sprite, virtual_size: Size) -> Beat {
    match graded.run_phase {
        RunPhase::Won => Beat::Won,
        RunPhase::GameOver => Beat::GameOver,
        RunPhase::Playing if graded.correct => Beat::Asking,
        RunPhase::Playing => Beat::Rejecting(
            Blink::new(cross.clone(), BannerAnchor::Center, virtual_size)
                .scale(1)
                .pattern(3, 8, 8),
        ),
    }
}

/// The whole quiz as one screen: it owns the answer field, the current beat, and
/// the three pre-baked phase banners, and drives the [`Quiz`] as input lands.
/// `tick` pumps the active beat (the reject cross to completion, the win marquee
/// as it scrolls); `handle` grades answers and restarts from a terminal beat;
/// `collect_layers` picks the layers for the beat.
struct QuizScreen {
    quiz: Quiz,
    input: InputField,
    beat: Beat,
    cross: Sprite,
    game_over: Placard,
    win: Marquee,
    virtual_size: Size,
}

impl Screen<Ctx> for QuizScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        let asking = matches!(self.beat, Beat::Asking);
        match input {
            UiInput::Char(ch) if asking => self.input.type_char(ch),
            UiInput::Backspace if asking => self.input.backspace(),
            UiInput::Confirm if asking => {
                let graded = self.quiz.answer(&self.input.submit());
                self.beat = next_beat(graded, &self.cross, self.virtual_size);
                if matches!(self.beat, Beat::Asking) {
                    self.input.set_prompt(self.quiz.prompt());
                }
            }
            // From a terminal beat (win / game over), Enter restarts the run.
            UiInput::Confirm if self.quiz.phase() != RunPhase::Playing => {
                self.quiz.reset();
                self.input.set_prompt(self.quiz.prompt());
                self.beat = Beat::Asking;
            }
            UiInput::Cancel => ctx.quit = true,
            _ => {}
        }
        ScreenChange::None
    }

    fn tick(&mut self, _ctx: &mut Ctx) -> ScreenChange<Ctx> {
        // Pump the reject cross to completion, scroll the win marquee. The borrow
        // of `beat` ends before it may be reassigned.
        let mut reject_done = false;
        match &mut self.beat {
            Beat::Rejecting(blink) => {
                blink.advance();
                reject_done = blink.is_done();
            }
            Beat::Won => self.win.advance(),
            _ => {}
        }
        if reject_done {
            self.input.set_prompt(self.quiz.prompt());
            self.beat = Beat::Asking;
        }
        ScreenChange::None
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        match &self.beat {
            Beat::Asking => overlays.push(&self.input),
            Beat::Rejecting(blink) => {
                overlays.push(&self.input);
                overlays.push(blink);
            }
            Beat::GameOver => world.push(&self.game_over),
            Beat::Won => world.push(&self.win),
        }
    }
}

/// The one durable bit of state the host loop watches.
#[derive(Default)]
struct Ctx {
    quit: bool,
}

fn main() -> Result<()> {
    let (config_path, _) = parse_config_flag(std::env::args().skip(1))?;
    let config = ConfigSource::resolve(config_path).load()?;

    // One system font for the AA input overlay, another for the pixel-art banner
    // glyphs (`SystemFont` isn't `Clone`; loading twice is cheap).
    let input_font = SystemFont::load(&config.input.font)?;
    let glyphs = RasterGlyphSource::new(SystemFont::load(&config.input.font)?, BANNER_CELL_PX);

    // Bake the three phase banners once. The reject cross and GAME OVER sign are
    // flat / gold-shadowed; the win text reuses the marquee's configured palette.
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
    .sprite(&glyphs);
    let game_over = Placard::new(
        Banner {
            text: "GAME OVER".to_string(),
            scale: 1,
            tracking: 1,
            shadow_depth: 3,
            outline_px: 1,
            gap: 6,
            colors: TextColors {
                fill: palette::WARNING,
                outline: palette::OUTLINE,
                shadow: palette::SHADOW, // gold 3D extrusion
            },
        }
        .sprite(&glyphs),
    );
    let win = Marquee::new(
        Banner {
            text: "YOU WIN".to_string(),
            scale: 2,
            tracking: 1,
            shadow_depth: 3,
            outline_px: 1,
            gap: 6,
            colors: config.marquee.colors,
        }
        .sprite(&glyphs),
        config.marquee.speed,
    );

    let quiz = Quiz::new(&rules(), questions())?;
    let input = InputField::new(config.input.clone(), input_font).with_prompt(quiz.prompt());

    // The host owns the window, framebuffer, and per-frame loop; hand it a ready
    // presentation over the configured (fixed) virtual screen.
    let screen = config.screen;
    let presentation = Presentation::new(
        screen.size,
        screen.backdrop,
        screen.letterbox,
        screen.min_scale,
    );
    let mut host = MinifbHost::new(&config.window, presentation)?;
    let mut stack = ScreenStack::new(Box::new(QuizScreen {
        quiz,
        input,
        beat: Beat::Asking,
        cross,
        game_over,
        win,
        virtual_size: screen.size,
    }));
    let mut ctx = Ctx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}

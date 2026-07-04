//! `math_game` — a worked example wiring the ratgames toolkit into a tiny quiz.
//!
//! This is the canonical *consumer* shape. A small example-local quiz
//! ([`quiz`]) supplies the game's content — questions and grading — and drives a
//! reusable [`ratgames::GameRun`] for the arcade sequencing (lives, levels,
//! score, win / game over). The presentation is composed from generic ratgames
//! layers: the anti-aliased [`InputField`], a flashing [`Blink`] reject cross, a
//! [`Placard`] GAME OVER sign, and a scrolling [`Marquee`] win banner, all
//! composited by [`Presentation`]. Nothing math-specific lives in the library.
//!
//! Run with `cargo run --example math_game --features minifb`; type an answer,
//! Enter submits, Backspace edits, Esc (or close) quits. From the win / game-over
//! screen, Enter restarts. Pass `--config <file>` to load a TOML/JSON `Config`
//! for the window / screen / input styling.

mod banner;
mod quiz;

use anyhow::Result;
use minifb::{InputCallback, Key, KeyRepeat, Window, WindowOptions};
use ratgames::{
    BannerAnchor, Blink, ConfigSource, DeviceClass, GameRules, InputField, Marquee, OverlayLayer,
    PixelLayer, Placard, Presentation, RasterGlyphSource, RunPhase, Size, Sprite, Surface,
    SystemFont, TextColors, palette, parse_config_flag,
};
use std::sync::mpsc::{self, Receiver, Sender};

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

/// Forwards unicode input from the window into a channel drained each frame. A
/// channel (not `Rc<RefCell>`) keeps the `'static` callback decoupled from the
/// loop's owned state.
struct CharSink(Sender<char>);

impl InputCallback for CharSink {
    fn add_char(&mut self, uni_char: u32) {
        if let Some(ch) = char::from_u32(uni_char) {
            let _ = self.0.send(ch);
        }
    }
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
    let cross_sprite = Banner {
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
    let mut win = Marquee::new(
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

    let mut quiz = Quiz::new(&rules(), questions())?;
    let mut input = InputField::new(config.input.clone(), input_font).with_prompt(quiz.prompt());
    let mut beat = Beat::Asking;

    // Window, sized responsively from config: a DeviceClass preset, or an explicit
    // width/height override.
    let w = &config.window;
    let init = w.size();
    let mut window = Window::new(
        &w.title,
        init.w as usize,
        init.h as usize,
        WindowOptions {
            resize: w.resizable,
            ..WindowOptions::default()
        },
    )?;
    window.set_target_fps(w.target_fps);

    let (tx, rx): (Sender<char>, Receiver<char>) = mpsc::channel();
    window.set_input_callback(Box::new(CharSink(tx)));

    // Composition target. The virtual screen tracks the window's device class, so
    // resizing across a breakpoint swaps the surface (rebuilt in the loop below).
    let screen = config.screen;
    let (mut win_w, mut win_h) = window.get_size();
    let mut class = DeviceClass::for_width(win_w as u32);
    let mut presentation = Presentation::new(
        screen.size_for(class),
        screen.backdrop,
        screen.letterbox,
        screen.min_scale,
    );
    let mut framebuffer = Surface::new(Size::new(win_w as u32, win_h as u32), screen.letterbox);

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let (nw, nh) = window.get_size();
        if (nw, nh) != (win_w, win_h) {
            win_w = nw;
            win_h = nh;
            framebuffer = Surface::new(Size::new(win_w as u32, win_h as u32), screen.letterbox);
            let new_class = DeviceClass::for_width(win_w as u32);
            if new_class != class {
                class = new_class;
                presentation = Presentation::new(
                    screen.size_for(class),
                    screen.backdrop,
                    screen.letterbox,
                    screen.min_scale,
                );
            }
        }

        // Drain typed characters every frame so keystrokes during a beat don't
        // buffer for the next round; only feed the field while asking.
        for ch in rx.try_iter() {
            if matches!(beat, Beat::Asking) {
                input.type_char(ch);
            }
        }
        for key in window.get_keys_pressed(KeyRepeat::Yes) {
            let asking = matches!(beat, Beat::Asking);
            match key {
                Key::Backspace if asking => input.backspace(),
                Key::Enter if asking => {
                    let graded = quiz.answer(&input.submit());
                    beat = next_beat(graded, &cross_sprite, presentation.virtual_size());
                    if matches!(beat, Beat::Asking) {
                        input.set_prompt(quiz.prompt());
                    }
                }
                // From a terminal beat (win / game over), Enter restarts the run.
                Key::Enter if quiz.phase() != RunPhase::Playing => {
                    quiz.reset();
                    input.set_prompt(quiz.prompt());
                    beat = Beat::Asking;
                }
                _ => {}
            }
        }

        // Advance the active beat: pump the reject cross to completion, scroll the
        // win marquee. The borrow of `beat` ends before it may be reassigned.
        let mut reject_done = false;
        match &mut beat {
            Beat::Rejecting(blink) => {
                blink.advance();
                reject_done = blink.is_done();
            }
            Beat::Won => win.advance(),
            _ => {}
        }
        if reject_done {
            input.set_prompt(quiz.prompt());
            beat = Beat::Asking;
        }

        // Compose the frame: pick the pixel-art layer and overlays by beat.
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        match &beat {
            Beat::Asking => overlays.push(&input),
            Beat::Rejecting(blink) => {
                overlays.push(&input);
                overlays.push(blink);
            }
            Beat::GameOver => world.push(&game_over),
            Beat::Won => world.push(&win),
        }
        presentation.render(&world, &overlays, &mut framebuffer);
        window.update_with_buffer(framebuffer.as_slice(), win_w, win_h)?;
    }

    Ok(())
}

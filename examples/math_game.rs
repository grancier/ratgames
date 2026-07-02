//! `math_game` — a worked example wiring the ratgames toolkit into a tiny quiz.
//!
//! Nothing new is built here; it only composes library parts. An [`InputField`]
//! with a prompt asks "What is 6+6?" inside the light-blue nested-border panel;
//! [`Quiz`] grades the answer and runs the retry loop; a red [`Placard`] "X"
//! blinks on a miss, a "GAME OVER" placard lingers, then it loops back to the
//! question; a correct "12" settles on the green "YOU WIN" [`Marquee`] from the
//! banner demo. Every colour, size, and timing comes from [`Config`].
//!
//! Run with `cargo run --example math_game`. Type an answer, Enter to submit,
//! Backspace to edit, Esc (or close) to quit.

use anyhow::Result;
use minifb::{InputCallback, Key, KeyRepeat, Window, WindowOptions};
use ratgames::{
    BigText, Config, InputField, Marquee, OverlayLayer, Phase, PixelLayer, Placard, Presentation,
    Quiz, Size, Surface, SystemFont,
};
use std::sync::mpsc::{self, Receiver, Sender};

/// Forwards unicode input from the window into a channel drained each frame.
/// A channel (not `Rc<RefCell>`) keeps the 'static callback decoupled from the
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
    let config = Config::default();

    // Pixel-art layers, all baked from config — no literals here.
    let cross = Placard::new(config.quiz.cross.sprite());
    let game_over = Placard::new(config.quiz.game_over.sprite());
    let win_banner = BigText::new(config.marquee.text_scale)
        .tracking(config.marquee.tracking)
        .shadow_depth(config.marquee.shadow_depth)
        .gap(config.marquee.gap)
        .colors(config.marquee.colors)
        .build(&config.quiz.win_text);
    let mut win = Marquee::new(win_banner, config.marquee.speed);

    // The quiz rules.
    let mut quiz = Quiz::from_config(&config.quiz);

    // Overlay: the input field, its prompt tinted to match the light-blue
    // border (config-driven, so still no literal in the wiring).
    let font = SystemFont::load(&config.input.font)?;
    let mut input_cfg = config.input.clone();
    input_cfg.prompt_color = input_cfg.border.color;
    let mut input = InputField::new(input_cfg, font).with_prompt(quiz.prompt());

    // Composition target.
    let screen = config.screen;
    let mut presentation =
        Presentation::new(screen.size, screen.backdrop, screen.letterbox, screen.min_scale);

    // Window.
    let w = &config.window;
    let mut window = Window::new(
        &w.title,
        w.width as usize,
        w.height as usize,
        WindowOptions {
            resize: w.resizable,
            ..WindowOptions::default()
        },
    )?;
    window.set_target_fps(w.target_fps);

    let (tx, rx): (Sender<char>, Receiver<char>) = mpsc::channel();
    window.set_input_callback(Box::new(CharSink(tx)));

    let (mut win_w, mut win_h) = window.get_size();
    let mut framebuffer = Surface::new(Size::new(win_w as u32, win_h as u32), screen.letterbox);

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let (nw, nh) = window.get_size();
        if (nw, nh) != (win_w, win_h) {
            win_w = nw;
            win_h = nh;
            framebuffer = Surface::new(Size::new(win_w as u32, win_h as u32), screen.letterbox);
        }

        // Input is live only while the quiz is asking; otherwise drain the queue
        // so keystrokes during an animation don't buffer up for the next round.
        if quiz.is_asking() {
            for ch in rx.try_iter() {
                input.type_char(ch);
            }
            for key in window.get_keys_pressed(KeyRepeat::Yes) {
                match key {
                    Key::Backspace => input.backspace(),
                    Key::Enter => {
                        let answer = input.submit();
                        quiz.submit(&answer);
                    }
                    _ => {}
                }
            }
        } else {
            for _ in rx.try_iter() {}
        }

        quiz.advance();
        if quiz.phase() == Phase::Won {
            win.advance();
        }

        // Select the pixel-art layer for the current phase.
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        match quiz.phase() {
            Phase::Asking => {}
            Phase::Rejecting => {
                if quiz.cross_visible() {
                    world.push(&cross);
                }
            }
            Phase::GameOver => world.push(&game_over),
            Phase::Won => world.push(&win),
        }

        // The input field stays up except on the victory screen.
        let overlays: Vec<&dyn OverlayLayer> = if quiz.phase() == Phase::Won {
            Vec::new()
        } else {
            vec![&input]
        };

        presentation.render(&world, &overlays, &mut framebuffer);
        window.update_with_buffer(framebuffer.as_slice(), win_w, win_h)?;
    }

    Ok(())
}

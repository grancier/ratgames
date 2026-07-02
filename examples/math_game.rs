//! `math_game` — a worked example wiring the ratgames toolkit into a tiny quiz.
//!
//! The composition — which layer shows in which phase, the retry loop, the win
//! banner — lives in [`MathGame`]; this binary is just the window and the event
//! pump. Every colour, size, and timing comes from [`Config`].
//!
//! Run with `cargo run --example math_game`. Type an answer, Enter to submit,
//! Backspace to edit, Esc (or close) to quit.

use anyhow::Result;
use minifb::{InputCallback, Key, KeyRepeat, Window, WindowOptions};
use ratgames::{Config, MathGame, Presentation, Size, Surface, SystemFont};
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

    // The whole game — rules, layers, phase→layer policy — behind one type.
    let font = SystemFont::load(&config.input.font)?;
    let mut game = MathGame::new(&config, font);

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

        // The game gates input on its own phase; drain the queue every frame so
        // keystrokes during an animation don't buffer up for the next round.
        for ch in rx.try_iter() {
            game.type_char(ch);
        }
        for key in window.get_keys_pressed(KeyRepeat::Yes) {
            match key {
                Key::Backspace => game.backspace(),
                Key::Enter => game.submit(),
                _ => {}
            }
        }

        game.tick();
        game.render(&mut presentation, &mut framebuffer);
        window.update_with_buffer(framebuffer.as_slice(), win_w, win_h)?;
    }

    Ok(())
}

//! ratgames binary: window + event loop. All rendering lives in the library;
//! this wires config → layers → presentation and pumps input.

use anyhow::Result;
use minifb::{InputCallback, Key, KeyRepeat, Window, WindowOptions};
use ratgames::{
    Config, InputField, Marquee, OverlayLayer, PixelLayer, Presentation, Size, Surface, SystemFont,
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
    let text = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "YOU WIN!!".to_string());

    // Pixel-art world: the marquee banner, through the configured glyph source.
    let banner = config.marquee.text_sprite(&text)?;
    let mut marquee = Marquee::new(banner, config.marquee.speed);

    // Overlay: the input field, using a resolved system font.
    let font = SystemFont::load(&config.input.font)?;
    let mut input = InputField::new(config.input.clone(), font);

    // Composition target.
    let screen = config.screen;
    let mut presentation = Presentation::new(
        screen.size,
        screen.backdrop,
        screen.letterbox,
        screen.min_scale,
    );

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

        // Text input: printable chars via the callback, edit keys via polling.
        for ch in rx.try_iter() {
            input.type_char(ch);
        }
        for key in window.get_keys_pressed(KeyRepeat::Yes) {
            match key {
                Key::Backspace => input.backspace(),
                Key::Enter => {
                    input.submit();
                }
                _ => {}
            }
        }

        marquee.advance();

        let world: [&dyn PixelLayer; 1] = [&marquee];
        let overlays: [&dyn OverlayLayer; 1] = [&input];
        presentation.render(&world, &overlays, &mut framebuffer);

        window.update_with_buffer(framebuffer.as_slice(), win_w, win_h)?;
    }

    Ok(())
}

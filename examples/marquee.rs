//! `marquee` — the ratgames marquee demo: a scrolling oversized-text banner over
//! an anti-aliased input field, in a native framebuffer window.
//!
//! `ratgames` is a library; this is a consumer of it. Config comes from the
//! built-in defaults, or a `--config <file>` TOML/JSON file (e.g.
//! `examples/marquee.toml` / `examples/marquee.json`); an optional positional
//! argument overrides the banner text. Run with `cargo run --example marquee`.

use anyhow::Result;
use minifb::{InputCallback, Key, KeyRepeat, Window, WindowOptions};
use ratgames::{
    ConfigSource, DeviceClass, InputField, Marquee, OverlayLayer, PixelLayer, Presentation, Size,
    Surface, SystemFont, parse_config_flag,
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
    let (config_path, positionals) = parse_config_flag(std::env::args().skip(1))?;
    let config = ConfigSource::resolve(config_path).load()?;
    let text = positionals
        .into_iter()
        .next()
        .unwrap_or_else(|| "YOU WIN!!".to_string());

    // Pixel-art world: the marquee banner, through the configured glyph source.
    let banner = config.marquee.text_sprite(&text)?;
    let mut marquee = Marquee::new(banner, config.marquee.speed);

    // Overlay: the input field, using a resolved system font.
    let font = SystemFont::load(&config.input.font)?;
    let mut input = InputField::new(config.input.clone(), font);

    // Window, sized responsively from config: a DeviceClass preset, or an
    // explicit width/height override.
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
            // Adapt the virtual screen when the window crosses a breakpoint.
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

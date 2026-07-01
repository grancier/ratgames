//! ratgames binary: open a window and run the marquee through the presentation
//! pipeline. All rendering logic lives in the library; this is just wiring and
//! the event loop.

use minifb::{Key, Window, WindowOptions};
use ratgames::{
    palette, BigText, Marquee, OverlayLayer, PixelLayer, Presentation, Size, Surface,
    VIRTUAL_SCREEN,
};

/// Initial window size; 3× of the 256² screen. Resizable — the presentation
/// re-fits every frame.
const INIT_W: usize = 768;
const INIT_H: usize = 768;

fn main() -> Result<(), minifb::Error> {
    let text = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "YOU WIN!!".to_string());

    let banner = BigText::new(6).build(&text);
    let mut marquee = Marquee::new(banner, 2);
    let mut presentation = Presentation::new(VIRTUAL_SCREEN, palette::BG, palette::LETTERBOX);

    let mut window = Window::new(
        "ratgames",
        INIT_W,
        INIT_H,
        WindowOptions {
            resize: true,
            ..WindowOptions::default()
        },
    )?;
    window.set_target_fps(60);

    let (mut w, mut h) = window.get_size();
    let mut framebuffer = Surface::new(Size::new(w as u32, h as u32), palette::LETTERBOX);

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let (nw, nh) = window.get_size();
        if (nw, nh) != (w, h) {
            w = nw;
            h = nh;
            framebuffer = Surface::new(Size::new(w as u32, h as u32), palette::LETTERBOX);
        }

        marquee.advance();

        // No overlay layers are wired yet — the input line is stubbed. Adding
        // one is `let overlays: [&dyn OverlayLayer; 1] = [&input_line];`.
        let world: [&dyn PixelLayer; 1] = [&marquee];
        let overlays: [&dyn OverlayLayer; 0] = [];
        presentation.render(&world, &overlays, &mut framebuffer);

        window.update_with_buffer(framebuffer.as_slice(), w, h)?;
    }

    Ok(())
}

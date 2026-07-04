//! The native `minifb` host backend.
//!
//! [`MinifbHost`] owns the window, the [`Presentation`], the device-pixel
//! framebuffer, and the backend-key → [`UiInput`] mapping. Its API deals only in
//! ratgames types — [`UiInput`] on the way in, [`PixelLayer`]/[`OverlayLayer`]
//! on the way out, a generic [`ScreenStack`] to drive — so it runs any game
//! without knowing anything about the consumer's rules.
//!
//! For the common case — drive a [`ScreenStack`] until the window closes or the
//! game asks to quit — hand [`run`](MinifbHost::run) the stack, the shared
//! context, and a quit predicate:
//!
//! ```text
//! let mut host = MinifbHost::new(&config.window, presentation)?;
//! host.run(&mut stack, &mut session, |s| s.quit)?;
//! ```
//!
//! A game that needs extra per-frame work can still drive the loop by hand with
//! the primitives [`is_open`](MinifbHost::is_open),
//! [`poll_inputs`](MinifbHost::poll_inputs), and [`render`](MinifbHost::render):
//!
//! ```text
//! while host.is_open() {
//!     for input in host.poll_inputs() {
//!         stack.handle(input, &mut session);
//!     }
//!     stack.tick(&mut session);
//!     stack.collect_layers(&session, &mut world, &mut overlays);
//!     host.render(&world, &overlays)?;
//! }
//! ```

use std::sync::mpsc::{self, Receiver, Sender};

use ::minifb::{InputCallback, Key, KeyRepeat, Window, WindowOptions};

use crate::color::Color;
use crate::config::WindowConfig;
use crate::geometry::Size;
use crate::present::{OverlayLayer, PixelLayer, Presentation};
use crate::session::ScreenStack;
use crate::surface::Surface;
use crate::ui::UiInput;

/// An error from the `minifb` window backend.
#[derive(Debug, thiserror::Error)]
pub enum HostError {
    /// The window could not be created.
    #[error("failed to open the window")]
    Open(#[source] ::minifb::Error),
    /// The framebuffer could not be presented to the window.
    #[error("failed to present the framebuffer")]
    Present(#[source] ::minifb::Error),
}

/// Forwards unicode input from the window into a channel drained each frame. A
/// channel (not `Rc<RefCell>`) keeps the `'static` callback decoupled from the
/// host's owned state.
struct CharSink(Sender<char>);

impl InputCallback for CharSink {
    fn add_char(&mut self, uni_char: u32) {
        if let Some(ch) = char::from_u32(uni_char) {
            let _ = self.0.send(ch);
        }
    }
}

/// The native window host: window + [`Presentation`] + framebuffer + input pump.
///
/// Construct it with a [`WindowConfig`] and a ready [`Presentation`]; then each
/// frame drain [`poll_inputs`](MinifbHost::poll_inputs), drive your own screen
/// stack, and hand the resulting layers to [`render`](MinifbHost::render).
pub struct MinifbHost {
    window: Window,
    presentation: Presentation,
    framebuffer: Surface,
    chars: Receiver<char>,
}

impl MinifbHost {
    /// Open a window sized from `window` and drive `presentation` into it.
    pub fn new(window: &WindowConfig, presentation: Presentation) -> Result<Self, HostError> {
        let init = window.size();
        let mut win = Window::new(
            &window.title,
            init.w as usize,
            init.h as usize,
            WindowOptions {
                resize: window.resizable,
                ..WindowOptions::default()
            },
        )
        .map_err(HostError::Open)?;
        win.set_target_fps(window.target_fps);

        let (tx, rx): (Sender<char>, Receiver<char>) = mpsc::channel();
        win.set_input_callback(Box::new(CharSink(tx)));

        let framebuffer = Surface::new(init, Color::rgb(0, 0, 0));
        Ok(Self {
            window: win,
            presentation,
            framebuffer,
            chars: rx,
        })
    }

    /// Whether the window is still open (the user has not closed it).
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.window.is_open()
    }

    /// Drain this frame's input as semantic [`UiInput`] commands: printable
    /// characters (via the char callback) followed by the control keys pressed
    /// this frame. A digit or letter is never double-counted — it arrives only
    /// as a [`UiInput::Char`], because [`ui_input_from_key`] maps control keys
    /// only.
    pub fn poll_inputs(&mut self) -> Vec<UiInput> {
        let mut inputs = Vec::new();
        for ch in self.chars.try_iter() {
            if !ch.is_control() {
                inputs.push(UiInput::Char(ch));
            }
        }
        for key in self.window.get_keys_pressed(KeyRepeat::Yes) {
            if let Some(input) = ui_input_from_key(key) {
                inputs.push(input);
            }
        }
        inputs
    }

    /// Composite `world`/`overlays` through the presentation and upload the
    /// frame, adapting the framebuffer to the current (possibly resized) window.
    pub fn render(
        &mut self,
        world: &[&dyn PixelLayer],
        overlays: &[&dyn OverlayLayer],
    ) -> Result<(), HostError> {
        let (w, h) = self.window.get_size();
        let size = Size::new(w as u32, h as u32);
        if self.framebuffer.size() != size {
            self.framebuffer = Surface::new(size, Color::rgb(0, 0, 0));
        }
        self.presentation
            .render(world, overlays, &mut self.framebuffer);
        self.window
            .update_with_buffer(self.framebuffer.as_slice(), w, h)
            .map_err(HostError::Present)
    }

    /// Run the game loop over `stack` until the window closes or `should_quit`
    /// reports the game is done, driving the shared context `ctx` through each
    /// frame: drain input to the stack, tick it, then composite and present its
    /// layers. This is the whole per-frame loop, so a game that fits the
    /// poll → tick → render shape writes no loop of its own; one that needs extra
    /// per-frame work drives the primitives ([`is_open`](Self::is_open),
    /// [`poll_inputs`](Self::poll_inputs), [`render`](Self::render)) by hand.
    ///
    /// `should_quit` is checked before each frame, so a `ctx` flag set while
    /// handling input ends the loop without drawing another frame.
    ///
    /// # Errors
    /// [`HostError::Present`] if a frame cannot be uploaded to the window.
    pub fn run<C>(
        &mut self,
        stack: &mut ScreenStack<C>,
        ctx: &mut C,
        should_quit: impl Fn(&C) -> bool,
    ) -> Result<(), HostError> {
        while self.is_open() && !should_quit(ctx) {
            for input in self.poll_inputs() {
                stack.handle(input, ctx);
            }
            stack.tick(ctx);

            let mut world: Vec<&dyn PixelLayer> = Vec::new();
            let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
            stack.collect_layers(ctx, &mut world, &mut overlays);
            self.render(&world, &overlays)?;
        }
        Ok(())
    }
}

/// Map a backend control key to its semantic [`UiInput`]. Character keys (letters,
/// digits, space) return `None` — printable text arrives through the char
/// callback instead, so a key is never double-counted as both a key and a
/// character.
fn ui_input_from_key(key: Key) -> Option<UiInput> {
    Some(match key {
        Key::Enter => UiInput::Confirm,
        Key::Backspace => UiInput::Backspace,
        Key::Delete => UiInput::Delete,
        Key::Escape => UiInput::Cancel,
        Key::Left => UiInput::Left,
        Key::Right => UiInput::Right,
        Key::Up => UiInput::Up,
        Key::Down => UiInput::Down,
        Key::Home => UiInput::Home,
        Key::End => UiInput::End,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_keys_map_to_semantic_inputs() {
        assert_eq!(ui_input_from_key(Key::Enter), Some(UiInput::Confirm));
        assert_eq!(ui_input_from_key(Key::Backspace), Some(UiInput::Backspace));
        assert_eq!(ui_input_from_key(Key::Delete), Some(UiInput::Delete));
        assert_eq!(ui_input_from_key(Key::Escape), Some(UiInput::Cancel));
        assert_eq!(ui_input_from_key(Key::Left), Some(UiInput::Left));
        assert_eq!(ui_input_from_key(Key::Right), Some(UiInput::Right));
        assert_eq!(ui_input_from_key(Key::Up), Some(UiInput::Up));
        assert_eq!(ui_input_from_key(Key::Down), Some(UiInput::Down));
        assert_eq!(ui_input_from_key(Key::Home), Some(UiInput::Home));
        assert_eq!(ui_input_from_key(Key::End), Some(UiInput::End));
    }

    #[test]
    fn character_and_unmapped_keys_return_none() {
        // Letters, digits, and space arrive as typed characters, not control
        // mappings, so the key mapper leaves them for the char callback.
        assert_eq!(ui_input_from_key(Key::A), None);
        assert_eq!(ui_input_from_key(Key::Key5), None);
        assert_eq!(ui_input_from_key(Key::Space), None);
        assert_eq!(ui_input_from_key(Key::Tab), None);
    }
}

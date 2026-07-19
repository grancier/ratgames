//! The WebAssembly `<canvas>` host backend.
//!
//! [`WasmHost`] is the browser sibling of
//! [`MinifbHost`](crate::host::minifb::MinifbHost): it owns the
//! [`Presentation`], the device-pixel framebuffer, and the browser-key →
//! [`UiInput`] mapping, and it deals only in ratgames types. The one structural
//! difference is the frame loop. A browser cannot block, so there is no
//! `run(...)`: the loop lives in JavaScript (`requestAnimationFrame`), which
//! calls back into a small `#[wasm_bindgen]` shell the *consumer* owns. That
//! shell forwards each frame's input and drives one [`frame`](WasmHost::frame):
//!
//! ```text
//! // once, on init: hand the shell a canvas and a ready Presentation.
//! let mut host = WasmHost::new(canvas, presentation)?;
//!
//! // per keydown (in JS): translate and queue.
//! if let Some(input) = ui_input_from_key(&event.key()) { pending.push(input); }
//!
//! // per animation frame (from JS): drain input, drive the stack, present.
//! host.frame(&mut stack, &mut ctx, &pending)?;
//! pending.clear();
//! ```
//!
//! Rendering is identical to the native path: the [`Presentation`] does the CPU
//! integer-upscale + letterbox into a device-sized [`Surface`], and the host
//! swizzles that `0x00RRGGBB` buffer to `RGBA8` and blits it with a single
//! `putImageData`. Keeping the upscale in the [`Presentation`] means the browser
//! frame is pixel-identical to the native one.

use crate::ui::UiInput;

/// Translate a browser [`KeyboardEvent.key`] string into a semantic [`UiInput`].
///
/// Named control keys map to their command; any single-character `key` (the
/// browser reports the typed character directly, e.g. `"a"`, `"5"`, `" "`)
/// becomes a [`UiInput::Char`]. Multi-character names that carry no command —
/// `"Shift"`, `"Tab"`, `"F1"`, `"Dead"`, … — return `None`. One `keydown`
/// therefore yields at most one `UiInput`, so a character is never double-counted
/// as both a key and a char (the hazard the native backend dodges with a
/// separate char callback).
///
/// [`KeyboardEvent.key`]: https://developer.mozilla.org/docs/Web/API/KeyboardEvent/key
#[must_use]
pub fn ui_input_from_key(key: &str) -> Option<UiInput> {
    Some(match key {
        "Enter" => UiInput::Confirm,
        "Backspace" => UiInput::Backspace,
        "Delete" => UiInput::Delete,
        "Escape" => UiInput::Cancel,
        "ArrowLeft" => UiInput::Left,
        "ArrowRight" => UiInput::Right,
        "ArrowUp" => UiInput::Up,
        "ArrowDown" => UiInput::Down,
        "Home" => UiInput::Home,
        "End" => UiInput::End,
        // A single non-control character is typed text; anything else is a named
        // key we do not model.
        other => {
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if !c.is_control() => UiInput::Char(c),
                _ => return None,
            }
        }
    })
}

/// Pack a `0x00RRGGBB` framebuffer into `RGBA8` bytes for a canvas `ImageData`.
///
/// The [`Surface`](crate::surface::Surface) stores opaque `0x_0_0RRGGBB` words
/// (alpha dropped, as the native path also relies on); a canvas `ImageData` is a
/// little-endian `[R, G, B, A]` byte stream. `out` is cleared and refilled so the
/// host reuses one scratch buffer across frames.
fn write_rgba8(pixels: &[u32], out: &mut Vec<u8>) {
    out.clear();
    out.reserve(pixels.len() * 4);
    for &px in pixels {
        out.push((px >> 16) as u8); // R
        out.push((px >> 8) as u8); // G
        out.push(px as u8); // B
        out.push(0xFF); // A (opaque)
    }
}

pub use canvas::{WasmHost, WasmHostError};

mod canvas {
    use wasm_bindgen::{Clamped, JsCast};
    use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData};

    use super::write_rgba8;
    use crate::color::Color;
    use crate::geometry::Size;
    use crate::present::{OverlayLayer, PixelLayer, Presentation};
    use crate::session::ScreenStack;
    use crate::surface::Surface;
    use crate::ui::UiInput;

    /// An error from the canvas host backend.
    #[derive(Debug, thiserror::Error)]
    pub enum WasmHostError {
        /// The canvas would not yield a 2D rendering context.
        #[error("the canvas has no 2d rendering context")]
        NoContext,
        /// A canvas operation (building or blitting `ImageData`) failed.
        #[error("canvas operation failed: {0}")]
        Canvas(String),
    }

    /// The browser `<canvas>` host: a 2D context + [`Presentation`] + framebuffer.
    ///
    /// Construct it once with the target canvas and a ready [`Presentation`],
    /// then drive one [`frame`](WasmHost::frame) per animation frame. The device
    /// framebuffer tracks the canvas backing-store size (`canvas.width/height`),
    /// so the consumer's shell owns responsiveness by resizing the canvas; the
    /// host adapts on the next frame.
    pub struct WasmHost {
        canvas: HtmlCanvasElement,
        ctx: CanvasRenderingContext2d,
        presentation: Presentation,
        framebuffer: Surface,
        rgba: Vec<u8>,
    }

    impl WasmHost {
        /// Bind to `canvas` and drive `presentation` into its 2D context.
        ///
        /// # Errors
        /// [`WasmHostError::NoContext`] if the canvas exposes no 2D context, and
        /// [`WasmHostError::Canvas`] if requesting the context throws.
        pub fn new(
            canvas: HtmlCanvasElement,
            presentation: Presentation,
        ) -> Result<Self, WasmHostError> {
            let ctx = canvas
                .get_context("2d")
                .map_err(|e| WasmHostError::Canvas(format!("{e:?}")))?
                .ok_or(WasmHostError::NoContext)?
                .dyn_into::<CanvasRenderingContext2d>()
                .map_err(|_| WasmHostError::NoContext)?;
            let framebuffer = Surface::new(canvas_size(&canvas), Color::rgb(0, 0, 0));
            Ok(Self {
                canvas,
                ctx,
                presentation,
                framebuffer,
                rgba: Vec::new(),
            })
        }

        /// Composite `world`/`overlays` and blit the frame to the canvas,
        /// adapting the framebuffer to the current canvas backing-store size.
        ///
        /// # Errors
        /// [`WasmHostError::Canvas`] if `ImageData` construction or `putImageData`
        /// throws.
        pub fn render(
            &mut self,
            world: &[&dyn PixelLayer],
            overlays: &[&dyn OverlayLayer],
        ) -> Result<(), WasmHostError> {
            let size = canvas_size(&self.canvas);
            if self.framebuffer.size() != size {
                self.framebuffer = Surface::new(size, Color::rgb(0, 0, 0));
            }
            self.presentation
                .render(world, overlays, &mut self.framebuffer);

            write_rgba8(self.framebuffer.as_slice(), &mut self.rgba);
            let image = ImageData::new_with_u8_clamped_array_and_sh(
                Clamped(self.rgba.as_slice()),
                size.w,
                size.h,
            )
            .map_err(|e| WasmHostError::Canvas(format!("{e:?}")))?;
            self.ctx
                .put_image_data(&image, 0.0, 0.0)
                .map_err(|e| WasmHostError::Canvas(format!("{e:?}")))
        }

        /// Drive one frame of `stack` over the shared context `ctx`: apply this
        /// frame's `inputs`, tick, then composite and blit the stack's layers.
        /// The browser mirror of [`MinifbHost::run`](crate::host::minifb::MinifbHost::run)'s
        /// per-frame body — the consumer's `requestAnimationFrame` shell calls it
        /// once per frame with the input queued since the last one.
        ///
        /// # Errors
        /// [`WasmHostError::Canvas`] if the frame cannot be blitted.
        pub fn frame<C>(
            &mut self,
            stack: &mut ScreenStack<C>,
            ctx: &mut C,
            inputs: &[UiInput],
        ) -> Result<(), WasmHostError> {
            for &input in inputs {
                stack.handle(input, ctx);
            }
            stack.tick(ctx);

            let mut world: Vec<&dyn PixelLayer> = Vec::new();
            let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
            stack.collect_layers(ctx, &mut world, &mut overlays);
            self.render(&world, &overlays)
        }
    }

    /// The canvas backing-store size in device pixels.
    fn canvas_size(canvas: &HtmlCanvasElement) -> Size {
        Size::new(canvas.width(), canvas.height())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_control_keys_map_to_semantic_inputs() {
        assert_eq!(ui_input_from_key("Enter"), Some(UiInput::Confirm));
        assert_eq!(ui_input_from_key("Backspace"), Some(UiInput::Backspace));
        assert_eq!(ui_input_from_key("Delete"), Some(UiInput::Delete));
        assert_eq!(ui_input_from_key("Escape"), Some(UiInput::Cancel));
        assert_eq!(ui_input_from_key("ArrowLeft"), Some(UiInput::Left));
        assert_eq!(ui_input_from_key("ArrowRight"), Some(UiInput::Right));
        assert_eq!(ui_input_from_key("ArrowUp"), Some(UiInput::Up));
        assert_eq!(ui_input_from_key("ArrowDown"), Some(UiInput::Down));
        assert_eq!(ui_input_from_key("Home"), Some(UiInput::Home));
        assert_eq!(ui_input_from_key("End"), Some(UiInput::End));
    }

    #[test]
    fn single_characters_become_typed_text() {
        assert_eq!(ui_input_from_key("a"), Some(UiInput::Char('a')));
        assert_eq!(ui_input_from_key("5"), Some(UiInput::Char('5')));
        assert_eq!(ui_input_from_key(" "), Some(UiInput::Char(' ')));
        assert_eq!(ui_input_from_key("?"), Some(UiInput::Char('?')));
    }

    #[test]
    fn unmodelled_named_keys_and_empties_return_none() {
        // Multi-character names carry no command we model; an empty string and a
        // lone control character are likewise dropped.
        assert_eq!(ui_input_from_key("Shift"), None);
        assert_eq!(ui_input_from_key("Tab"), None);
        assert_eq!(ui_input_from_key("F1"), None);
        assert_eq!(ui_input_from_key("Dead"), None);
        assert_eq!(ui_input_from_key(""), None);
        assert_eq!(ui_input_from_key("\u{7}"), None);
    }

    #[test]
    fn rgba8_swizzles_argb_words_to_opaque_rgba_bytes() {
        let mut out = Vec::new();
        // 0x00RRGGBB: red, green, blue.
        write_rgba8(&[0x00FF_0000, 0x0000_FF00, 0x0000_00FF], &mut out);
        assert_eq!(
            out,
            vec![
                0xFF, 0x00, 0x00, 0xFF, // R
                0x00, 0xFF, 0x00, 0xFF, // G
                0x00, 0x00, 0xFF, 0xFF, // B
            ]
        );
    }

    #[test]
    fn rgba8_clears_prior_contents_each_call() {
        let mut out = vec![1, 2, 3];
        write_rgba8(&[0x0012_3456], &mut out);
        assert_eq!(out, vec![0x12, 0x34, 0x56, 0xFF]);
    }
}

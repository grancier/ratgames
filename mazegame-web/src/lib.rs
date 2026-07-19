//! `mazegame-web` — the WebAssembly entry point for mazegame.
//!
//! A thin `#[wasm_bindgen]` shell over the shared `mazegame_app` construction:
//! it builds the browser-adapted game (the bundled config with the crate's
//! embedded font), owns a [`ratgames::WasmHost`], and exposes just enough to JS
//! to run the `requestAnimationFrame` loop the browser owns — feed a key, drive
//! a frame, ask whether to stop. All the game rules, screens, and rendering are
//! the same code the native binary runs; only the loop and the canvas differ.
//!
//! The whole crate is wasm-only: on a native host it compiles to nothing (so
//! the workspace build never drags minifb — or any windowing — onto wasm).
#![cfg(target_arch = "wasm32")]

use mazegame_app::{AppConfig, Ctx, PlayScreen, presentation};
use ratgames::{ScreenStack, UiInput, WasmHost, ui_input_from_key};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

/// Fallback seed when the shell passes no usable clock value.
const STARTER_SEED: u64 = 0x4D41_5A45; // "MAZE"

/// The live browser game: the canvas host, the screen stack, the durable
/// context, and the input queued since the last frame. JavaScript holds one of
/// these across the animation-frame loop.
#[wasm_bindgen]
pub struct Game {
    host: WasmHost,
    stack: ScreenStack<Ctx>,
    ctx: Ctx,
    /// Inputs collected from `keydown` since the last [`frame`](Game::frame).
    pending: Vec<UiInput>,
}

#[wasm_bindgen]
impl Game {
    /// Queue a browser `KeyboardEvent.key` for the next frame (control keys and
    /// single typed characters map to a [`UiInput`]; anything else is ignored).
    pub fn on_key(&mut self, key: &str) {
        if let Some(input) = ui_input_from_key(key) {
            self.pending.push(input);
        }
    }

    /// Drive one frame: apply the queued input, tick the game, and blit to the
    /// canvas. The queue is cleared whether or not the blit succeeds.
    ///
    /// # Errors
    /// A `JsValue` carrying the host error message if the frame cannot be
    /// presented.
    pub fn frame(&mut self) -> Result<(), JsValue> {
        let result = self
            .host
            .frame(&mut self.stack, &mut self.ctx, &self.pending)
            .map_err(to_js);
        self.pending.clear();
        result
    }

    /// Whether the player asked to quit (Esc). The shell stops the loop on this.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn quit(&self) -> bool {
        self.ctx.quit
    }
}

/// Build the browser game bound to `canvas`, seeding its mazes from `seed` (the
/// shell passes `Date.now()`, since wasm has no wall clock). The canvas's
/// backing-store size (`canvas.width`/`height`) is the device resolution the
/// frame is composited at; the shell owns sizing it.
///
/// # Errors
/// A `JsValue` carrying the message if the config, glyph source, canvas
/// context, or first maze deal fails.
#[wasm_bindgen]
pub fn start(canvas: HtmlCanvasElement, seed: f64) -> Result<Game, JsValue> {
    console_error_panic_hook::set_once();

    let config = AppConfig::bundled_for_web().map_err(to_js)?;
    let glyphs = config.glyphs.resolve().map_err(to_js)?;
    let ctx = Ctx::new(&config, glyphs, seed_to_u64(seed)).map_err(to_js)?;
    let host = WasmHost::new(canvas, presentation(&config)).map_err(to_js)?;
    let stack: ScreenStack<Ctx> = ScreenStack::new(Box::new(PlayScreen::new(&ctx)));

    Ok(Game {
        host,
        stack,
        ctx,
        pending: Vec::new(),
    })
}

/// `Date.now()` is a non-negative integer count of milliseconds; take its bits
/// as the seed, falling back to a fixed value if it is not a usable number.
fn seed_to_u64(seed: f64) -> u64 {
    if seed.is_finite() && seed >= 0.0 {
        seed as u64
    } else {
        STARTER_SEED
    }
}

/// Carry any error to JS as its display string.
fn to_js(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

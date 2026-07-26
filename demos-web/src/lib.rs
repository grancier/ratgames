//! `demos-web` — the WebAssembly entry point for the example gallery.
//!
//! A thin `#[wasm_bindgen]` shell over the shared `demos` crate: [`start`]
//! builds the named demo with the crate-bundled embedded fonts (a browser has
//! no system font database), owns a [`ratgames::WasmHost`], and exposes just
//! enough to JS to run the loop the browser owns — feed a key, drive a frame,
//! ask whether to stop, and whether the demo animates continuously (a ticking
//! marquee or wave) or only changes on input (so the shell renders on demand,
//! the turn-based-web lesson). All the screens, layers, and values are the
//! same code the native example binaries run; only the loop, the fonts, and
//! the canvas differ.
//!
//! The whole crate is wasm-only: on a native host it compiles to nothing (so
//! the workspace build never drags any windowing onto wasm).
#![cfg(target_arch = "wasm32")]

use demos::room_scroll::RoomScrollDemo;
use demos::{DemoCtx, math_game};
use ratgames::{
    Config, FontConfig, FontSource, FontWeight, HighScoreStore, HighScores, MemoryHighScoreStore,
    Presentation, Screen, ScreenStack, UiInput, WasmHost, ui_input_from_key,
};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

/// The demos that animate continuously (a ticking layer: the scrolling
/// marquee, the rippling wave, the room slide, the feedback beat); the rest
/// change only on input, so their shell renders on demand.
fn is_animated(name: &str) -> bool {
    matches!(name, "marquee" | "text_wave" | "room_scroll" | "math_game")
}

/// The default font, resolved to the crate-bundled face at **bold** — the
/// established web ruling (the same one mazegame's build made): regular-weight
/// DejaVu thresholds too thin at 1-bit, so the browser always renders the
/// heavier face.
fn embedded_font() -> FontConfig {
    FontConfig {
        source: FontSource::Embedded {
            weight: FontWeight(700),
        },
        ..FontConfig::default()
    }
}

/// The default config with every font swapped for the crate-bundled bold face
/// — the browser counterpart of the native defaults (the marquee's default
/// glyph source is the font-free 8×8 bitmap, which the swap leaves unchanged).
fn embedded_config() -> Config {
    let mut config = Config::default();
    config.input.font = embedded_font();
    config.marquee.glyph_source = config.marquee.glyph_source.with_embedded_font();
    config
}

/// The compositor for a config-driven demo's virtual screen.
fn presentation_from(config: &Config) -> Presentation {
    let s = &config.screen;
    Presentation::new(s.size, s.backdrop, s.letterbox, s.min_scale)
}

/// The bundled starting board — the same JSON the native example reads from
/// disk, parsed once at build time into the in-memory store's seed.
fn sample_scores() -> Result<HighScores, serde_json::Error> {
    serde_json::from_str(include_str!("../../examples/high_scores.json"))
}

/// The running demo behind the JS handle: most demos are a screen stack over
/// the crate's shared [`DemoCtx`]; `math_game` carries its own richer context;
/// `room_scroll` is driven through its primitives (its room view borrows the
/// world per frame, so it is not a screen).
enum Running {
    Simple {
        stack: ScreenStack<DemoCtx>,
        ctx: DemoCtx,
    },
    Math {
        stack: ScreenStack<math_game::Ctx>,
        /// Boxed: the math context (field, glyph source, quiz) dwarfs the
        /// other variants.
        ctx: Box<math_game::Ctx>,
    },
    Rooms {
        demo: RoomScrollDemo,
        quit: bool,
    },
}

/// A screen-stack demo over the shared [`DemoCtx`].
fn simple(screen: impl Screen<DemoCtx> + 'static) -> Running {
    Running::Simple {
        stack: ScreenStack::new(Box::new(screen)),
        ctx: DemoCtx::default(),
    }
}

/// Build the named demo — its compositor and its running state — with the
/// embedded fonts. An unknown name is an error the shell shows.
fn build_demo(name: &str) -> Result<(Presentation, Running), JsValue> {
    Ok(match name {
        "arrow_nav" => (
            demos::arrow_nav::presentation(),
            simple(demos::arrow_nav::build()),
        ),
        "text_wave" => (
            demos::text_wave::presentation(),
            simple(
                demos::text_wave::build(&FontSource::Embedded {
                    weight: FontWeight(700),
                })
                .map_err(to_js)?,
            ),
        ),
        "level_complete" => (
            demos::level_complete::presentation(),
            simple(demos::level_complete::build(&embedded_font()).map_err(to_js)?),
        ),
        "text_input" => (
            demos::text_input::presentation(),
            simple(demos::text_input::build(&embedded_font()).map_err(to_js)?),
        ),
        "high_score" => {
            // The board loads through the storage seam, like the native main —
            // only the backend differs: an in-memory store seeded with the
            // bundled dummy data (a production build would put a service-backed
            // store here).
            let store: Box<dyn HighScoreStore> = Box::new(MemoryHighScoreStore::seeded(
                sample_scores().map_err(to_js)?,
            ));
            let scores = store.load().map_err(to_js)?;
            (
                demos::high_score::presentation(),
                simple(demos::high_score::build(&scores, &embedded_font()).map_err(to_js)?),
            )
        }
        "marquee" => {
            let config = embedded_config();
            let screen = demos::marquee::build(&config, "YOU WIN!!").map_err(to_js)?;
            (presentation_from(&config), simple(screen))
        }
        "math_game" => {
            let config = embedded_config();
            let ctx = math_game::context(&config).map_err(to_js)?;
            let stack = ScreenStack::new(math_game::challenge_screen(&ctx));
            (
                presentation_from(&config),
                Running::Math {
                    stack,
                    ctx: Box::new(ctx),
                },
            )
        }
        "room_scroll" => (
            demos::room_scroll::presentation(),
            Running::Rooms {
                demo: RoomScrollDemo::new(),
                quit: false,
            },
        ),
        other => return Err(JsValue::from_str(&format!("unknown demo: {other}"))),
    })
}

/// The live browser demo: the canvas host, the running demo, and the input
/// queued since the last frame. JavaScript holds one of these across its loop.
#[wasm_bindgen]
pub struct Demo {
    host: WasmHost,
    running: Running,
    /// Inputs collected from `keydown` since the last [`frame`](Demo::frame).
    pending: Vec<UiInput>,
    animated: bool,
}

#[wasm_bindgen]
impl Demo {
    /// Queue a browser `KeyboardEvent.key` for the next frame (control keys and
    /// single typed characters map to a [`UiInput`]; anything else is ignored).
    pub fn on_key(&mut self, key: &str) {
        if let Some(input) = ui_input_from_key(key) {
            self.pending.push(input);
        }
    }

    /// Drive one frame: apply the queued input, tick the demo, and blit to the
    /// canvas. The queue is cleared whether or not the blit succeeds.
    ///
    /// # Errors
    /// A `JsValue` carrying the host error message if the frame cannot be
    /// presented.
    pub fn frame(&mut self) -> Result<(), JsValue> {
        let result = match &mut self.running {
            Running::Simple { stack, ctx } => {
                self.host.frame(stack, ctx, &self.pending).map_err(to_js)
            }
            Running::Math { stack, ctx } => self
                .host
                .frame(stack, ctx.as_mut(), &self.pending)
                .map_err(to_js),
            Running::Rooms { demo, quit } => {
                for &input in &self.pending {
                    if matches!(input, UiInput::Cancel) {
                        *quit = true;
                    } else {
                        demo.steer(input);
                    }
                }
                demo.advance();
                demo.with_layers(|layers| self.host.render(layers, &[]))
                    .map_err(to_js)
            }
        };
        self.pending.clear();
        result
    }

    /// Whether the player asked to quit (Esc). The shell stops the loop on this.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn quit(&self) -> bool {
        match &self.running {
            Running::Simple { ctx, .. } => ctx.quit,
            Running::Math { ctx, .. } => ctx.quit,
            Running::Rooms { quit, .. } => *quit,
        }
    }

    /// Whether the demo animates continuously; a `false` demo changes only on
    /// input, so the shell renders on demand instead of running a 60 fps loop.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn animated(&self) -> bool {
        self.animated
    }

    /// The width of the virtual screen the demo renders at, in pixels. The
    /// shell sizes the canvas from this so the picture fills it at the right
    /// aspect.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn width(&self) -> u32 {
        self.host.virtual_size().w
    }

    /// The height of the virtual screen the demo renders at, in pixels.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn height(&self) -> u32 {
        self.host.virtual_size().h
    }
}

/// Build the demo named `name` bound to `canvas`. The canvas's backing-store
/// size (`canvas.width`/`height`) is the device resolution the frame is
/// composited at; the shell owns sizing it.
///
/// # Errors
/// A `JsValue` carrying the message if the name is unknown, a font or the
/// board fails to materialise, or the canvas has no 2D context.
#[wasm_bindgen]
pub fn start(name: &str, canvas: HtmlCanvasElement) -> Result<Demo, JsValue> {
    console_error_panic_hook::set_once();

    let animated = is_animated(name);
    let (presentation, running) = build_demo(name)?;
    let host = WasmHost::new(canvas, presentation).map_err(to_js)?;

    Ok(Demo {
        host,
        running,
        pending: Vec::new(),
        animated,
    })
}

/// Carry any error to JS as its display string.
fn to_js(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&error.to_string())
}

//! `mazegame-app` as a library: the game construction shared by every host.
//!
//! The native binary (`main.rs`, minifb) and the WebAssembly shell
//! (`mazegame-web`, canvas) both need the *same* game: load the config, resolve
//! the glyph source, deal the first rung into a [`Ctx`], and stack a
//! [`PlayScreen`] on it. That wiring lives here so neither host duplicates it,
//! and — crucially — this library is **windowing-agnostic**: it pulls no
//! `ratgames` host backend, so a wasm consumer builds it with no native
//! windowing. Only `main.rs` reaches for `MinifbHost`, behind the `minifb`
//! feature (on by default; a wasm consumer turns it off).

pub mod config;
mod scene;
pub mod screens;

use ratgames::Presentation;

pub use config::{AppConfig, AppConfigError};
pub use screens::{Ctx, PlayScreen};

/// The compositor for `config`'s virtual screen — the integer-upscale +
/// letterbox is identical on every host, so both the window and the canvas
/// backend drive the same [`Presentation`].
#[must_use]
pub fn presentation(config: &AppConfig) -> Presentation {
    let screen = &config.engine.screen;
    Presentation::new(
        screen.size,
        screen.backdrop,
        screen.letterbox,
        screen.min_scale,
    )
}

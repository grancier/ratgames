//! Optional windowing backends that drive a
//! [`Presentation`](crate::present::Presentation) into a live window.
//!
//! ratgames' core is windowing-agnostic — it only produces `Vec<u32>` frames. A
//! host backend is an *optional* adapter (compiled only when its feature is
//! enabled) that owns the window, the frame pump, the framebuffer upload, and
//! the backend-key → [`UiInput`](crate::ui::UiInput) mapping, so a game reuses
//! one loop instead of re-hand-rolling it while the core stays portable.
//!
//! Enable the `minifb` feature for the native `minifb` backend, or the `wasm`
//! feature for the browser `<canvas>` backend.

#[cfg(feature = "minifb")]
pub mod minifb;
#[cfg(feature = "wasm")]
pub mod wasm;

#[cfg(feature = "minifb")]
pub use minifb::{HostError, MinifbHost};
#[cfg(feature = "wasm")]
pub use wasm::{WasmHost, WasmHostError, ui_input_from_key};

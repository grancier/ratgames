//! demos — the ratgames example gallery as a library.
//!
//! Every demo's screens, layers, and construction live here **once**, shared by
//! the two hosts that play them: the native example binaries
//! (`ratgames/examples/*.rs`, thin mains over `MinifbHost`) and the browser
//! build (`demos-web`, a `WasmHost` canvas shell). On the `mazegame-app`
//! pattern the crate is windowing-agnostic — it pulls no ratgames host backend
//! — so the wasm consumer builds it with no native windowing.
//!
//! Composition and values (texts, colours, sizes, the question bank, the room
//! map) belong to the demos and stay here; reusable *mechanism* belongs to
//! `ratgames` and is only consumed. Font *sources* are the callers' choice —
//! constructors take a font selector ([`ratgames::FontConfig`] /
//! [`ratgames::FontSource`]) so the native mains pass system faces and the web
//! shell passes the crate-bundled embedded ones.

pub mod arrow_nav;
pub mod high_score;
pub mod level_complete;
pub mod marquee;
pub mod math_game;
pub mod room_scroll;
pub mod text_input;
pub mod text_wave;

/// The one durable bit of state the simple demos' host loop watches. (The
/// `math_game` demo carries a richer per-demo context of its own.)
#[derive(Debug, Default)]
pub struct DemoCtx {
    /// Set when the player asks to leave (Esc); the host loop stops on it.
    pub quit: bool,
}

/// Why a demo could not be built: its font failed to load, its config failed
/// to materialise, or its fixed rules failed validation.
#[derive(Debug, thiserror::Error)]
pub enum DemoError {
    #[error(transparent)]
    Font(#[from] ratgames::FontError),
    #[error(transparent)]
    Config(#[from] ratgames::ConfigError),
    #[error(transparent)]
    Rules(#[from] ratgames::GameRulesError),
}

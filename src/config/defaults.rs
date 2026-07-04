//! Product-string defaults, sourced from the bundled `defaults.json` rather than
//! hardcoded in Rust literals.
//!
//! The window title is the one bit of *product* content in the library's
//! defaults; it lives in a data file that `Config` reads. The JSON is embedded at
//! compile time via `include_str!` and parsed once, lazily, into
//! [`DEFAULT_STRINGS`]; a unit test parses it too, so a malformed bundle is a
//! build-time failure, not a runtime risk.

use std::sync::LazyLock;

/// The default product strings, grouped to mirror the config sections that use
/// them.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct DefaultStrings {
    pub(crate) window: WindowStrings,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct WindowStrings {
    pub(crate) title: String,
}

/// The bundled default strings, parsed once on first use.
pub(crate) static DEFAULT_STRINGS: LazyLock<DefaultStrings> = LazyLock::new(|| {
    serde_json::from_str(include_str!("defaults.json"))
        .expect("bundled config/defaults.json must be valid")
});

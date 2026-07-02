//! Reusable UI layer over the `Surface` / `PixelLayer` / `OverlayLayer`
//! boundaries.
//!
//! The pieces here are deliberately backend- and windowing-agnostic:
//!
//! * [`UiInput`] — a semantic input command a backend (minifb in the examples)
//!   maps its raw keys onto, so one widget works under any backend.
//! * [`Panel`] — a filled, bordered frame with a content rect (the reusable
//!   generalisation of the input panel's private layout).
//! * [`Label`] — aligned anti-aliased text drawn into a rect (device space).
//! * [`Menu`] — a selection list; [`MultipleChoice`] layers answer semantics on
//!   the same model. [`MenuView`] renders a menu as an [`OverlayLayer`].
//!
//! Every widget is constructed with typed params/builders (mirroring `BigText`,
//! `InputField`, `Marquee`) and draws into a caller-owned `Surface`. Colours and
//! sizes come from the caller (a `Theme`/`Config` supplies defaults) but a file
//! is never required — the front door is code, not config.

mod event;
mod label;
mod menu;
mod panel;
mod view;

pub use event::UiInput;
pub use label::{Align, Label};
pub use menu::{Menu, MultipleChoice};
pub use panel::Panel;
pub use view::{MenuView, stacked_rects};

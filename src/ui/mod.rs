//! Reusable UI layer over the `Surface` / `PixelLayer` / `OverlayLayer`
//! boundaries.
//!
//! The pieces here are deliberately backend- and windowing-agnostic:
//!
//! * [`UiInput`] — a semantic input command a backend (minifb in the examples)
//!   maps its raw keys onto, so one widget works under any backend.
//! * [`Panel`] — a filled frame with selectable [`Borders`], a content rect, and
//!   a title slot (the reusable generalisation of the input panel's layout).
//! * [`Label`] — aligned anti-aliased text in a rect (device space);
//!   [`Paragraph`] is the multi-line, word-wrapping form (prompts / lesson copy).
//! * [`Menu`] — a selection list; [`MultipleChoice`] layers answer semantics on
//!   the same model. [`MenuView`] renders a menu as an [`OverlayLayer`].
//! * [`split`] — divide a rect into child rects by [`Constraint`]s, the
//!   positioning complement to [`Panel::content_rect`].
//!
//! Every widget is constructed with typed params/builders (mirroring `BigText`,
//! `InputField`, `Marquee`) and draws into a caller-owned `Surface`. Colours and
//! sizes come from the caller (a `Theme`/`Config` supplies defaults) but a file
//! is never required — the front door is code, not config.

mod event;
mod label;
mod layout;
mod menu;
mod panel;
mod paragraph;
mod view;

pub use event::UiInput;
pub use label::{Align, Label};
pub use layout::{Axis, Constraint, split};
pub use menu::{Menu, MultipleChoice};
pub use panel::{Borders, Panel};
pub use paragraph::{Paragraph, wrap_lines};
pub use view::{MenuView, stacked_rects};

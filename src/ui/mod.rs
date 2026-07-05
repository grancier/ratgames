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
//!   the same model. [`MenuView`] renders a menu as anti-aliased labels;
//!   [`ChoiceList`] is the pixel-art, banner-backed counterpart (a caret-marked
//!   stack of [`ShadowBanner`]s), both [`OverlayLayer`](crate::OverlayLayer)s.
//! * [`ShadowBanner`] — pixel-art text (device space) with a real drop shadow; the
//!   [`OverlayLayer`](crate::OverlayLayer) counterpart to the pixel-space
//!   [`Placard`](crate::Placard),
//!   for banners whose shadow offset must be a fraction of a virtual pixel.
//! * [`Flash`] — a translucent full-viewport colour wash
//!   ([`OverlayLayer`](crate::OverlayLayer)); the arcade hit-flash / fade
//!   primitive, its strength driven by the caller.
//! * [`Blink`] — a sprite that flashes a fixed number of times over the viewport
//!   ([`OverlayLayer`](crate::OverlayLayer)); a reject X / warning glyph, its
//!   pattern pumped by the caller.
//! * [`Countdown`] — a frame-budget timer the caller pumps toward expiry (an
//!   auto-advancing card, a time limit); draws nothing, so pair it with whatever
//!   it times. [`CountdownConfig`] is its serde config.
//! * [`FeedbackBeat`] — the arcade answer-feedback beat: an opening [`Blink`] then
//!   a verdict [`ShadowBanner`] held on a [`Countdown`] under an optional fading
//!   [`Flash`] wash. Reports the per-phase [`FeedbackBeatLayers`] the caller
//!   composes with its own layers (it is not itself an
//!   [`OverlayLayer`](crate::OverlayLayer)).
//! * [`split`] — divide a rect into child rects by [`Constraint`]s, the
//!   positioning complement to [`Panel::content_rect`].
//!
//! Every widget is constructed with typed params/builders (mirroring `BigText`,
//! `InputField`, `Marquee`) and draws into a caller-owned `Surface`. Colours and
//! sizes come from the caller (a `Theme`/`Config` supplies defaults) but a file
//! is never required — the front door is code, not config.

mod answer_mode;
mod blink;
mod choice_list;
mod countdown;
mod event;
mod feedback_beat;
mod flash;
mod label;
mod layout;
mod menu;
mod panel;
mod paragraph;
mod shadow_banner;
mod view;

pub use answer_mode::{AnswerMode, AnswerModeError};
pub use blink::{Blink, BlinkConfig};
pub use choice_list::ChoiceList;
pub use countdown::{Countdown, CountdownConfig};
pub use event::UiInput;
pub use feedback_beat::{FeedbackBeat, FeedbackBeatLayers};
pub use flash::Flash;
pub use label::{Align, Label};
pub use layout::{Axis, Constraint, split};
pub use menu::{Menu, MultipleChoice};
pub use panel::{Borders, Panel};
pub use paragraph::{Paragraph, wrap_lines};
pub use shadow_banner::{
    BannerAnchor, ShadowBanner, ShadowBannerFactory, ShadowConfig, ShadowLength, ShadowStyle,
    bake_drop_shadow,
};
pub use view::{MenuView, stacked_rects};

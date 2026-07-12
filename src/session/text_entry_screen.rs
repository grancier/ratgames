//! [`TextEntryScreen`] — a reusable text-entry screen: the game's shared input
//! field, edited until the player submits or cancels.
//!
//! The arcade "type your name" screen. A game keeps one durable [`InputField`]
//! (it owns a system font, so it lives in the shared `Ctx`, not per screen);
//! this screen routes every editing event to it, commits the entered line on
//! [`Confirm`](UiInput::Confirm), and backs out on [`Cancel`](UiInput::Cancel).
//! Like [`TimedCard`](super::TimedCard) / [`PromptScreen`](super::PromptScreen),
//! where each exit *goes* is the game's business: one one-shot [`FnOnce`]
//! transition maps the [`TextEntryExit`] — carrying the submitted text — to a
//! [`ScreenChange`] with a fresh `&mut Ctx`.
//!
//! The screen reaches the field through the [`InputContext`] seam, the
//! text-entry counterpart of [`BannerContext`](super::BannerContext). The seam
//! exposes the editable [`InputLine`] (editing and submit are pure line
//! operations — [`InputField::handle`](crate::InputField::handle) and
//! [`submit`](crate::InputField::submit) just delegate) plus the drawn field as
//! an [`OverlayLayer`], so the mechanism never needs the field's font and stays
//! deterministic to test.
//!
//! [`InputField`]: crate::InputField

use super::{Screen, ScreenChange};
use crate::input::InputLine;
use crate::present::{OverlayLayer, PixelLayer};
use crate::ui::UiInput;

/// The seam handing a text-entry screen the game's shared input field: the
/// editable line (for routing editing input and committing the entered text)
/// and the drawn field (for rendering). The game's `Ctx` implements this over
/// the one durable [`InputField`](crate::InputField) it owns.
pub trait InputContext {
    /// The editable answer line of the game's shared field.
    fn input_line(&mut self) -> &mut InputLine;

    /// The drawn field — the overlay a frame composites — whose line
    /// [`input_line`](Self::input_line) edits.
    fn input_overlay(&self) -> &dyn OverlayLayer;
}

/// Why a [`TextEntryScreen`] is leaving: the player committed the entered text,
/// or backed out. The screen's transition maps this to a [`ScreenChange`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextEntryExit {
    /// The player pressed [`Confirm`](UiInput::Confirm); the entered line is
    /// committed here (and cleared for the next entry).
    Submitted(String),
    /// The player pressed [`Cancel`](UiInput::Cancel).
    Cancelled,
}

/// The one-shot transition a [`TextEntryScreen`] fires as it leaves.
type TextEntryTransition<Ctx> = Box<dyn FnOnce(TextEntryExit, &mut Ctx) -> ScreenChange<Ctx>>;

/// A text-entry [`Screen`]: the game's shared input field, edited until the
/// player submits ([`Confirm`](UiInput::Confirm)) or backs out
/// ([`Cancel`](UiInput::Cancel)), then a single transition routes onward with
/// the committed text. Construct with [`new`](Self::new) and push onto the
/// [`ScreenStack`](crate::ScreenStack); set the field's prompt before entry
/// (it is the game's field, not the screen's).
pub struct TextEntryScreen<Ctx> {
    on_exit: Option<TextEntryTransition<Ctx>>,
}

impl<Ctx> TextEntryScreen<Ctx> {
    /// A text-entry screen routing each exit through `on_exit`. The transition
    /// fires at most once, with a fresh `&mut Ctx`.
    #[must_use]
    pub fn new(
        on_exit: impl FnOnce(TextEntryExit, &mut Ctx) -> ScreenChange<Ctx> + 'static,
    ) -> Self {
        Self {
            on_exit: Some(Box::new(on_exit)),
        }
    }

    /// Fire the one-shot transition for `why`, or [`ScreenChange::None`] if it
    /// has already fired.
    fn exit(&mut self, why: TextEntryExit, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match self.on_exit.take() {
            Some(on_exit) => on_exit(why, ctx),
            None => ScreenChange::None,
        }
    }
}

impl<Ctx: InputContext> Screen<Ctx> for TextEntryScreen<Ctx> {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Confirm => {
                // Commit the entered line: take its text and clear it for the
                // next entry (the field's prompt is untouched) — the line half
                // of `InputField::submit`.
                let line = ctx.input_line();
                let text = line.text().to_string();
                line.clear();
                self.exit(TextEntryExit::Submitted(text), ctx)
            }
            UiInput::Cancel => self.exit(TextEntryExit::Cancelled, ctx),
            // Every other event is line editing (type, backspace, forward
            // delete, caret movement); the line ignores the ones it does not
            // own.
            other => {
                ctx.input_line().handle(other);
                ScreenChange::None
            }
        }
    }

    fn collect_layers<'a>(
        &'a self,
        ctx: &'a Ctx,
        _world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        overlays.push(ctx.input_overlay());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;
    use crate::surface::Surface;

    /// A stand-in for the drawn field — the seam only needs *an* overlay.
    struct FakeOverlay;

    impl OverlayLayer for FakeOverlay {
        fn render(&self, _window: &mut Surface, _viewport: Rect) {}
    }

    /// The fake game context: a real (font-free) [`InputLine`], a fake drawn
    /// field, and a log of the exits the screen fires.
    struct Ctx {
        line: InputLine,
        overlay: FakeOverlay,
        exits: Vec<TextEntryExit>,
    }

    impl Ctx {
        fn new() -> Self {
            Self {
                line: InputLine::new(),
                overlay: FakeOverlay,
                exits: Vec::new(),
            }
        }
    }

    impl InputContext for Ctx {
        fn input_line(&mut self) -> &mut InputLine {
            &mut self.line
        }

        fn input_overlay(&self) -> &dyn OverlayLayer {
            &self.overlay
        }
    }

    /// A screen that logs its exit reason.
    fn entry() -> TextEntryScreen<Ctx> {
        TextEntryScreen::new(|why, ctx: &mut Ctx| {
            ctx.exits.push(why);
            ScreenChange::None
        })
    }

    #[test]
    fn editing_input_reaches_the_shared_line() {
        let mut ctx = Ctx::new();
        let mut screen = entry();
        for c in "hi!".chars() {
            screen.handle(UiInput::Char(c), &mut ctx);
        }
        screen.handle(UiInput::Backspace, &mut ctx);
        assert_eq!(ctx.line.text(), "hi");
        assert!(ctx.exits.is_empty(), "editing never exits");
    }

    #[test]
    fn confirm_submits_the_entered_text_and_clears_the_line() {
        let mut ctx = Ctx::new();
        let mut screen = entry();
        for c in "42".chars() {
            screen.handle(UiInput::Char(c), &mut ctx);
        }
        screen.handle(UiInput::Confirm, &mut ctx);
        assert_eq!(ctx.exits, [TextEntryExit::Submitted("42".to_string())]);
        assert_eq!(ctx.line.text(), "", "cleared for the next entry");
    }

    #[test]
    fn cancel_exits_as_cancelled_and_leaves_the_line() {
        let mut ctx = Ctx::new();
        let mut screen = entry();
        screen.handle(UiInput::Char('x'), &mut ctx);
        screen.handle(UiInput::Cancel, &mut ctx);
        assert_eq!(ctx.exits, [TextEntryExit::Cancelled]);
        assert_eq!(ctx.line.text(), "x", "a cancel does not clear the entry");
    }

    #[test]
    fn the_transition_fires_at_most_once() {
        let mut ctx = Ctx::new();
        let mut screen = entry();
        screen.handle(UiInput::Confirm, &mut ctx);
        screen.handle(UiInput::Confirm, &mut ctx);
        screen.handle(UiInput::Cancel, &mut ctx);
        assert_eq!(
            ctx.exits,
            [TextEntryExit::Submitted(String::new())],
            "fires once"
        );
    }

    #[test]
    fn collect_layers_contributes_the_field_overlay() {
        let ctx = Ctx::new();
        let screen = entry();
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        screen.collect_layers(&ctx, &mut world, &mut overlays);
        assert_eq!(overlays.len(), 1);
        assert!(world.is_empty());
    }
}

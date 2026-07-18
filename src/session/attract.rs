//! [`AttractLoop`] — the arcade *attract mode*: the idle cabinet's looping showcase.
//!
//! Left untouched, an arcade cabinet slips into attract mode — a self-running
//! showcase (high scores, a how-to, a demo) that loops to draw a passer-by to the
//! controls. This is the reusable half of that: a set of [`AttractCard`]s shown
//! one at a time, each held for its own [`Countdown`], turning to the next and
//! wrapping past the last — forever — until *any* input dismisses the whole loop
//! through a single transition. The game supplies the cards and where waking
//! leads; `ratgames` owns the cycling and the rendering.
//!
//! Like [`TimedCard`](super::TimedCard) the loop *is* a [`Screen`], so a game
//! drops it onto the [`ScreenStack`](crate::ScreenStack) and hand-wires both ends
//! — the entry (typically an idle [`Countdown`] on the title screen) and the exit
//! — with no flow framework in between. The one difference from a `TimedCard`: a
//! card expiring never *leaves* the loop, it only turns the page; the loop is left
//! solely by input. The dismiss transition runs with a fresh `&mut Ctx` and fires
//! **once** (guarded by [`Option::take`]), so two inputs on the way out cannot
//! double-fire it.

use super::{Screen, ScreenChange};
use crate::present::{OverlayLayer, PixelLayer};
use crate::ui::{Countdown, CountdownConfig, ShadowBanner, UiInput};

/// One card in an [`AttractLoop`]: the banners to show and how long to hold them
/// before the loop turns to the next. The game bakes the banners in its own style
/// (a high-score board, a how-to, a title splash).
pub struct AttractCard {
    banners: Vec<ShadowBanner>,
    hold: Countdown,
}

impl AttractCard {
    /// A card showing `banners` for `hold`, after which the loop rotates on.
    #[must_use]
    pub fn new(banners: Vec<ShadowBanner>, hold: Countdown) -> Self {
        Self { banners, hold }
    }
}

/// The one-shot transition an [`AttractLoop`] fires when input wakes it: given
/// mutable session state, it returns the stack change to apply (typically back to
/// the title).
type AttractDismiss<Ctx> = Box<dyn FnOnce(&mut Ctx) -> ScreenChange<Ctx>>;

/// The arcade attract loop: [`AttractCard`]s shown in turn, each held for its own
/// [`Countdown`] then turning to the next (wrapping past the last, forever), until
/// *any* input dismisses the loop through a single transition. Construct with
/// [`new`](Self::new) and push onto the [`ScreenStack`](crate::ScreenStack); the
/// game supplies the cards and where waking leads.
///
/// A card's expiry only turns the page — it never leaves the loop; the loop ends
/// solely on input. A game enters it by hand (an idle countdown on its title),
/// keeping the toolkit out of routing.
pub struct AttractLoop<Ctx = ()> {
    cards: Vec<AttractCard>,
    current: usize,
    on_dismiss: Option<AttractDismiss<Ctx>>,
}

impl<Ctx> AttractLoop<Ctx> {
    /// A loop over `cards`, dismissed through `on_dismiss` on any input. The first
    /// card shows first; each holds for its own countdown then turns to the next,
    /// wrapping past the last back to the first. An empty `cards` shows nothing and
    /// simply waits for the input that dismisses it.
    #[must_use]
    pub fn new(
        cards: Vec<AttractCard>,
        on_dismiss: impl FnOnce(&mut Ctx) -> ScreenChange<Ctx> + 'static,
    ) -> Self {
        Self {
            cards,
            current: 0,
            on_dismiss: Some(Box::new(on_dismiss)),
        }
    }

    /// The index of the card currently showing (`0` before the first turn), or
    /// `None` when the loop has no cards.
    #[must_use]
    pub fn showing(&self) -> Option<usize> {
        (!self.cards.is_empty()).then_some(self.current)
    }

    /// Fire the one-shot dismiss transition, or [`ScreenChange::None`] if it has
    /// already fired — so a second input on the way out is a no-op.
    fn dismiss(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match self.on_dismiss.take() {
            Some(on_dismiss) => on_dismiss(ctx),
            None => ScreenChange::None,
        }
    }
}

impl<Ctx> Screen<Ctx> for AttractLoop<Ctx> {
    fn handle(&mut self, _input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        // Any sign of life wakes the cabinet — no key is special here.
        self.dismiss(ctx)
    }

    fn tick(&mut self, _ctx: &mut Ctx) -> ScreenChange<Ctx> {
        if let Some(card) = self.cards.get_mut(self.current) {
            card.hold.advance();
            if card.hold.is_expired() {
                // Re-arm the card we're leaving so it holds its full time when the
                // loop comes back to it, then turn to the next (wrapping). The card
                // we enter is already fresh — armed at construction, or re-armed
                // here when the loop last left it.
                card.hold.reset();
                self.current = (self.current + 1) % self.cards.len();
            }
        }
        ScreenChange::None
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        _world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        if let Some(card) = self.cards.get(self.current) {
            for banner in &card.banners {
                overlays.push(banner);
            }
        }
    }
}

/// Attract-mode timing: how long the title sits idle before the attract
/// rotation begins, and how long each [`AttractCard`] holds. Both are reusable
/// [`CountdownConfig`]s; a game's shipped values live in its config. An `idle`
/// of `0` frames (the default) disables attract mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AttractConfig {
    /// Title idle time before the rotation starts (`0` = never).
    pub idle: CountdownConfig,
    /// How long each attract card holds before rotating to the next.
    pub card: CountdownConfig,
}

impl Default for AttractConfig {
    fn default() -> Self {
        Self {
            idle: CountdownConfig { frames: 0 },
            card: CountdownConfig::default(),
        }
    }
}

/// Why an [`AttractConfig`] was rejected: attract mode turned on with a card
/// hold that would thrash the rotation every frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AttractConfigError {
    /// `idle.frames` armed the rotation but `card.frames` was zero.
    #[error("card.frames must be at least 1 when attract mode is on")]
    ZeroCardHold,
}

impl AttractConfig {
    /// The armed title-idle countdown, or `None` when attract mode is off.
    #[must_use]
    pub fn idle_countdown(&self) -> Option<Countdown> {
        (self.idle.frames > 0).then(|| self.idle.countdown())
    }

    /// Check an armed rotation holds each card: with attract mode on
    /// (`idle.frames > 0`), a zero card hold would turn the rotation every
    /// frame. Attract mode off (the default) has nothing to check.
    ///
    /// # Errors
    /// [`AttractConfigError::ZeroCardHold`] on an armed, hold-less rotation.
    pub fn validate(&self) -> Result<(), AttractConfigError> {
        if self.idle.frames > 0 && self.card.frames == 0 {
            return Err(AttractConfigError::ZeroCardHold);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Size};
    use crate::glyph::Bitmap8x8;
    use crate::ui::{ShadowBannerFactory, ShadowStyle};

    #[test]
    fn attract_config_validate_rejects_a_rotation_with_no_hold() {
        // Off (the default) has nothing to check.
        assert!(AttractConfig::default().validate().is_ok());
        // On with a zero card hold would thrash the rotation every frame.
        let thrashing = AttractConfig {
            idle: CountdownConfig { frames: 600 },
            card: CountdownConfig { frames: 0 },
        };
        assert_eq!(thrashing.validate(), Err(AttractConfigError::ZeroCardHold));
        // A zero card hold is fine while attract mode is off.
        let off = AttractConfig {
            idle: CountdownConfig { frames: 0 },
            card: CountdownConfig { frames: 0 },
        };
        assert!(off.validate().is_ok());
    }

    /// Counts how many times the loop was dismissed, proving the one-shot
    /// transition runs with `&mut Ctx` and fires exactly once.
    #[derive(Default)]
    struct Ctx {
        woke: u32,
    }

    /// `n` throwaway banners (their content is irrelevant to the loop's mechanic).
    fn banners(n: usize) -> Vec<ShadowBanner> {
        let factory =
            ShadowBannerFactory::new(&Bitmap8x8, ShadowStyle::default(), Size::new(64, 64));
        (0..n)
            .map(|i| factory.at("HI", Point::new(0, i as i32), 1))
            .collect()
    }

    /// A card of `n` banners held for `frames` frames.
    fn card(n: usize, frames: u32) -> AttractCard {
        AttractCard::new(banners(n), Countdown::new(frames))
    }

    /// A loop over `cards` that counts its dismissals.
    fn attract(cards: Vec<AttractCard>) -> AttractLoop<Ctx> {
        AttractLoop::new(cards, |ctx: &mut Ctx| {
            ctx.woke += 1;
            ScreenChange::None
        })
    }

    #[test]
    fn any_input_wakes_the_loop_through_the_transition() {
        let mut ctx = Ctx::default();
        let mut a = attract(vec![card(1, 10), card(1, 10)]);
        // Not just Confirm/Cancel — an arrow key is a sign of life too.
        assert!(matches!(
            a.handle(UiInput::Left, &mut ctx),
            ScreenChange::None
        ));
        assert_eq!(ctx.woke, 1);
    }

    #[test]
    fn the_dismiss_fires_at_most_once() {
        // Two inputs (and an intervening tick) on the way out must fire the
        // one-shot only once — the guard against a double-exit.
        let mut ctx = Ctx::default();
        let mut a = attract(vec![card(1, 10)]);
        a.handle(UiInput::Char('x'), &mut ctx);
        a.tick(&mut ctx);
        a.handle(UiInput::Confirm, &mut ctx);
        assert_eq!(ctx.woke, 1, "woke once");
    }

    #[test]
    fn cards_turn_on_each_hold_and_wrap_without_leaving() {
        let mut ctx = Ctx::default();
        let mut a = attract(vec![card(1, 2), card(1, 3)]); // holds of 2 and 3 frames
        assert_eq!(a.showing(), Some(0));

        a.tick(&mut ctx); // card 0: 1 of 2
        assert_eq!(a.showing(), Some(0));
        a.tick(&mut ctx); // card 0: 2 of 2 -> turn to card 1
        assert_eq!(a.showing(), Some(1));

        a.tick(&mut ctx); // card 1: 1 of 3
        a.tick(&mut ctx); // card 1: 2 of 3
        assert_eq!(a.showing(), Some(1));
        a.tick(&mut ctx); // card 1: 3 of 3 -> wrap to card 0
        assert_eq!(a.showing(), Some(0));

        assert_eq!(ctx.woke, 0, "turning the page never leaves the loop");
    }

    #[test]
    fn a_reentered_card_holds_its_full_time_again() {
        let mut ctx = Ctx::default();
        let mut a = attract(vec![card(1, 2), card(1, 1)]);
        // card 0 (2 frames) -> card 1 (1 frame) -> wrap back to card 0.
        for _ in 0..3 {
            a.tick(&mut ctx);
        }
        assert_eq!(a.showing(), Some(0), "wrapped back to the first card");

        // The re-entered card must hold its full two frames again, not expire on
        // the first tick — proof the loop re-armed it when it left.
        a.tick(&mut ctx);
        assert_eq!(a.showing(), Some(0), "still holding, one frame in");
        a.tick(&mut ctx);
        assert_eq!(a.showing(), Some(1), "and turns again after its full hold");
    }

    #[test]
    fn collect_layers_shows_only_the_current_card() {
        let mut ctx = Ctx::default();
        let mut a = attract(vec![card(2, 1), card(3, 1)]); // 2 then 3 banners

        {
            let mut world: Vec<&dyn PixelLayer> = Vec::new();
            let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
            a.collect_layers(&ctx, &mut world, &mut overlays);
            assert_eq!(overlays.len(), 2, "the first card's banners");
            assert!(world.is_empty(), "attract cards are device-space overlays");
        }

        a.tick(&mut ctx); // card 0: 1 of 1 -> turn to card 1

        {
            let mut world: Vec<&dyn PixelLayer> = Vec::new();
            let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
            a.collect_layers(&ctx, &mut world, &mut overlays);
            assert_eq!(overlays.len(), 3, "now only the second card's banners");
        }
    }

    #[test]
    fn an_empty_loop_shows_nothing_and_still_wakes_on_input() {
        let mut ctx = Ctx::default();
        let mut a = attract(vec![]);
        assert_eq!(a.showing(), None);

        a.tick(&mut ctx); // no card to turn: a no-op, not a panic
        assert_eq!(a.showing(), None);
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        a.collect_layers(&ctx, &mut world, &mut overlays);
        assert!(overlays.is_empty());

        a.handle(UiInput::Confirm, &mut ctx);
        assert_eq!(ctx.woke, 1, "input still dismisses an empty loop");
    }

    #[test]
    fn a_single_card_loop_re_holds_that_card() {
        let mut ctx = Ctx::default();
        let mut a = attract(vec![card(1, 2)]);
        a.tick(&mut ctx); // 1 of 2
        a.tick(&mut ctx); // 2 of 2 -> wraps to itself
        assert_eq!(a.showing(), Some(0));
        a.tick(&mut ctx); // fresh again: 1 of 2
        assert_eq!(a.showing(), Some(0));
        assert_eq!(ctx.woke, 0, "a lone card just re-holds, never leaving");
    }

    #[test]
    fn config_round_trips_and_arms_the_idle_trigger_only_when_on() {
        let config = AttractConfig::default();
        assert!(config.idle_countdown().is_none(), "attract off by default");
        let text = serde_json::to_string(&config).expect("serialize");
        let parsed: AttractConfig = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, config);

        let on = AttractConfig {
            idle: CountdownConfig { frames: 600 },
            card: CountdownConfig { frames: 300 },
        };
        assert_eq!(on.idle_countdown(), Some(Countdown::new(600)));
    }
}

//! A stack of screens — the session's state machine (the classic game-state
//! stack: title → menu → gameplay, with pause/dialog screens pushed on top).
//!
//! The top screen receives input and ticks each frame; screens beneath it freeze
//! (they keep their state but do not advance). Rendering composites from the
//! topmost *opaque* screen upward, so a transparent overlay (a dialog, a pause
//! menu) shows the frozen screen beneath it.
//!
//! Screens never mutate the stack directly — [`Screen::handle`] / [`Screen::tick`]
//! return a [`ScreenChange`] and the stack applies it. That keeps transitions
//! explicit and sidesteps the borrow checker (you cannot mutate the stack while a
//! screen it owns is borrowed).
//!
//! ## Shared context
//!
//! The stack is generic over a caller-owned context type `Ctx` (default `()`).
//! The application owns its durable session state — score, lives, player, the
//! problem in play, … — in a `Ctx` value and threads it through the screen
//! lifecycle: `&mut Ctx` into the mutating [`handle`](Screen::handle) /
//! [`tick`](Screen::tick), and `&Ctx` into the read-only
//! [`collect_layers`](Screen::collect_layers). ratgames never names or stores
//! that state; it only passes the one opaque slot along, so a screen reaches
//! shared services with plain typed access while the [`ScreenStack`] still owns
//! nothing but the screens themselves. A screen keeps only its *local* UI state
//! (cursor, selected index, input buffer, animation frame, cached view).

use crate::present::{OverlayLayer, PixelLayer};
use crate::ui::UiInput;

/// What a [`Screen`] asks the [`ScreenStack`] to do after an event.
pub enum ScreenChange<Ctx = ()> {
    /// Stay on the current screen.
    None,
    /// Push a new screen on top; this one keeps its state beneath it.
    Push(Box<dyn Screen<Ctx>>),
    /// Pop this screen, revealing the one beneath.
    Pop,
    /// Replace this screen with another (pop then push).
    Replace(Box<dyn Screen<Ctx>>),
}

/// One screen in the [`ScreenStack`]: a title, a menu, gameplay, a pause overlay.
///
/// A screen owns only its **local** UI state (cursor, selected index, input
/// buffer, animation frame, cached view). Durable session state lives in the
/// caller-owned `Ctx`, handed in by the stack: `&mut Ctx` to the mutating
/// [`handle`](Screen::handle) / [`tick`](Screen::tick), and `&Ctx` to the
/// read-only [`collect_layers`](Screen::collect_layers).
pub trait Screen<Ctx = ()> {
    /// Handle one input command; return the stack transition to apply. May
    /// mutate shared state through `ctx`.
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx>;

    /// Advance one frame. Called only on the top screen (covered screens freeze).
    /// May mutate shared state through `ctx`. Defaults to no change.
    fn tick(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        let _ = ctx;
        ScreenChange::None
    }

    /// Whether this screen fully covers those beneath it. Opaque (the default)
    /// occludes; a transparent screen lets the screen beneath show through.
    fn is_opaque(&self) -> bool {
        true
    }

    /// Contribute this screen's layers for the frame: pixel-art layers into
    /// `world`, device-space overlays into `overlays`, each drawn in push order.
    /// Reads shared state through `ctx` (rendering is read-only). Defaults to
    /// contributing nothing.
    fn collect_layers<'a>(
        &'a self,
        ctx: &'a Ctx,
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        let _ = (ctx, world, overlays);
    }
}

/// A stack of [`Screen`]s over a shared context `Ctx`. The top screen receives
/// input and ticks; rendering starts at the topmost opaque screen.
pub struct ScreenStack<Ctx = ()> {
    screens: Vec<Box<dyn Screen<Ctx>>>,
}

impl<Ctx> ScreenStack<Ctx> {
    /// A stack whose only (bottom) screen is `root`.
    #[must_use]
    pub fn new(root: Box<dyn Screen<Ctx>>) -> Self {
        Self {
            screens: vec![root],
        }
    }

    /// How many screens are stacked.
    #[must_use]
    pub fn len(&self) -> usize {
        self.screens.len()
    }

    /// Whether every screen has been popped.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.screens.is_empty()
    }

    /// Push a screen on top — for the app itself opening an overlay (a screen
    /// opens its own via [`ScreenChange::Push`]).
    pub fn push(&mut self, screen: Box<dyn Screen<Ctx>>) {
        self.screens.push(screen);
    }

    /// Pop the top screen, if any.
    pub fn pop(&mut self) {
        self.screens.pop();
    }

    /// Route `input` to the top screen (with mutable access to `ctx`) and apply
    /// the change it returns.
    pub fn handle(&mut self, input: UiInput, ctx: &mut Ctx) {
        if let Some(top) = self.screens.last_mut() {
            let change = top.handle(input, ctx);
            self.apply(change);
        }
    }

    /// Tick the top screen (covered screens freeze) with mutable access to `ctx`
    /// and apply the change.
    pub fn tick(&mut self, ctx: &mut Ctx) {
        if let Some(top) = self.screens.last_mut() {
            let change = top.tick(ctx);
            self.apply(change);
        }
    }

    /// Collect the visible layers bottom-to-top, starting at the topmost opaque
    /// screen so fully-covered screens are skipped. Hand the results to
    /// [`Presentation::render`](crate::present::Presentation::render).
    pub fn collect_layers<'a>(
        &'a self,
        ctx: &'a Ctx,
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        let start = self
            .screens
            .iter()
            .rposition(|s| s.is_opaque())
            .unwrap_or(0);
        for screen in &self.screens[start..] {
            screen.collect_layers(ctx, world, overlays);
        }
    }

    fn apply(&mut self, change: ScreenChange<Ctx>) {
        match change {
            ScreenChange::None => {}
            ScreenChange::Push(screen) => self.screens.push(screen),
            ScreenChange::Pop => {
                self.screens.pop();
            }
            ScreenChange::Replace(screen) => {
                self.screens.pop();
                self.screens.push(screen);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::surface::Surface;

    /// The test context: a log of `(screen id, event)` the screens append to,
    /// proving `&mut Ctx` reaches `handle`/`tick`.
    type Ctx = Vec<(u32, &'static str)>;

    struct FillLayer(Color);
    impl PixelLayer for FillLayer {
        fn render(&self, screen: &mut Surface) {
            screen.fill(self.0);
        }
    }

    struct Fake {
        id: u32,
        opaque: bool,
        layer: FillLayer,
    }

    impl Fake {
        fn boxed(id: u32, opaque: bool) -> Box<dyn Screen<Ctx>> {
            Box::new(Fake {
                id,
                opaque,
                layer: FillLayer(Color::rgb((id & 0xff) as u8, 0, 0)),
            })
        }
    }

    impl Screen<Ctx> for Fake {
        fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
            ctx.push((self.id, "handle"));
            match input {
                UiInput::Confirm => ScreenChange::Push(Fake::boxed(self.id * 10, true)),
                UiInput::Cancel => ScreenChange::Pop,
                _ => ScreenChange::None,
            }
        }
        fn tick(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
            ctx.push((self.id, "tick"));
            ScreenChange::None
        }
        fn is_opaque(&self) -> bool {
            self.opaque
        }
        fn collect_layers<'a>(
            &'a self,
            _ctx: &'a Ctx,
            world: &mut Vec<&'a dyn PixelLayer>,
            _overlays: &mut Vec<&'a dyn OverlayLayer>,
        ) {
            world.push(&self.layer);
        }
    }

    #[test]
    fn input_and_tick_reach_only_the_top() {
        let mut ctx: Ctx = Vec::new();
        let mut stack = ScreenStack::new(Fake::boxed(1, true));
        stack.handle(UiInput::Confirm, &mut ctx); // 1 handles, pushes 10
        assert_eq!(stack.len(), 2);
        ctx.clear();
        stack.tick(&mut ctx);
        assert_eq!(ctx, vec![(10, "tick")], "only the top ticks");
    }

    #[test]
    fn handle_and_tick_thread_the_shared_context() {
        let mut ctx: Ctx = Vec::new();
        let mut stack = ScreenStack::new(Fake::boxed(1, true));
        stack.handle(UiInput::Left, &mut ctx); // no transition, just records
        stack.tick(&mut ctx);
        assert_eq!(ctx, vec![(1, "handle"), (1, "tick")]);
    }

    #[test]
    fn push_pop_replace_move_the_stack() {
        let mut ctx: Ctx = Vec::new();
        let mut stack = ScreenStack::new(Fake::boxed(1, true));
        stack.handle(UiInput::Confirm, &mut ctx); // push 10
        assert_eq!(stack.len(), 2);
        stack.handle(UiInput::Cancel, &mut ctx); // top pops
        assert_eq!(stack.len(), 1);
        stack.push(Fake::boxed(2, true)); // app-driven push
        stack.pop();
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn rendering_starts_at_the_topmost_opaque_screen() {
        let ctx: Ctx = Vec::new();
        let mut stack = ScreenStack::new(Fake::boxed(1, true));
        stack.push(Fake::boxed(2, false)); // transparent overlay
        stack.push(Fake::boxed(3, true)); // opaque, covers 1 and 2

        // Scoped so the collected layer refs (borrowed from the stack) drop
        // before the mutating `pop` below.
        {
            let mut world: Vec<&dyn PixelLayer> = Vec::new();
            let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
            stack.collect_layers(&ctx, &mut world, &mut overlays);
            assert_eq!(world.len(), 1, "only the topmost opaque screen renders");
        }

        stack.pop(); // -> [opaque 1, transparent 2]
        {
            let mut world: Vec<&dyn PixelLayer> = Vec::new();
            let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
            stack.collect_layers(&ctx, &mut world, &mut overlays);
            assert_eq!(world.len(), 2, "opaque base shows through the overlay");
        }
    }
}

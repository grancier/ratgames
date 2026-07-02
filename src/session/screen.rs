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

use crate::present::{OverlayLayer, PixelLayer};
use crate::ui::UiInput;

/// What a [`Screen`] asks the [`ScreenStack`] to do after an event.
pub enum ScreenChange {
    /// Stay on the current screen.
    None,
    /// Push a new screen on top; this one keeps its state beneath it.
    Push(Box<dyn Screen>),
    /// Pop this screen, revealing the one beneath.
    Pop,
    /// Replace this screen with another (pop then push).
    Replace(Box<dyn Screen>),
}

/// One screen in the [`ScreenStack`]: a title, a menu, gameplay, a pause overlay.
pub trait Screen {
    /// Handle one input command; return the stack transition to apply.
    fn handle(&mut self, input: UiInput) -> ScreenChange;

    /// Advance one frame. Called only on the top screen (covered screens freeze).
    /// Defaults to no change.
    fn tick(&mut self) -> ScreenChange {
        ScreenChange::None
    }

    /// Whether this screen fully covers those beneath it. Opaque (the default)
    /// occludes; a transparent screen lets the screen beneath show through.
    fn is_opaque(&self) -> bool {
        true
    }

    /// Contribute this screen's layers for the frame: pixel-art layers into
    /// `world`, device-space overlays into `overlays`, each drawn in push order.
    /// Defaults to contributing nothing.
    fn collect_layers<'a>(
        &'a self,
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        let _ = (world, overlays);
    }
}

/// A stack of [`Screen`]s. The top screen receives input and ticks; rendering
/// starts at the topmost opaque screen.
pub struct ScreenStack {
    screens: Vec<Box<dyn Screen>>,
}

impl ScreenStack {
    /// A stack whose only (bottom) screen is `root`.
    #[must_use]
    pub fn new(root: Box<dyn Screen>) -> Self {
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
    pub fn push(&mut self, screen: Box<dyn Screen>) {
        self.screens.push(screen);
    }

    /// Pop the top screen, if any.
    pub fn pop(&mut self) {
        self.screens.pop();
    }

    /// Route `input` to the top screen and apply the change it returns.
    pub fn handle(&mut self, input: UiInput) {
        if let Some(top) = self.screens.last_mut() {
            let change = top.handle(input);
            self.apply(change);
        }
    }

    /// Tick the top screen (covered screens freeze) and apply the change.
    pub fn tick(&mut self) {
        if let Some(top) = self.screens.last_mut() {
            let change = top.tick();
            self.apply(change);
        }
    }

    /// Collect the visible layers bottom-to-top, starting at the topmost opaque
    /// screen so fully-covered screens are skipped. Hand the results to
    /// [`Presentation::render`](crate::present::Presentation::render).
    pub fn collect_layers<'a>(
        &'a self,
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        let start = self
            .screens
            .iter()
            .rposition(|s| s.is_opaque())
            .unwrap_or(0);
        for screen in &self.screens[start..] {
            screen.collect_layers(world, overlays);
        }
    }

    fn apply(&mut self, change: ScreenChange) {
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
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A shared log of `(screen id, event)` so tests can see routing.
    type Log = Rc<RefCell<Vec<(u32, &'static str)>>>;

    struct FillLayer(Color);
    impl PixelLayer for FillLayer {
        fn render(&self, screen: &mut Surface) {
            screen.fill(self.0);
        }
    }

    struct Fake {
        id: u32,
        opaque: bool,
        log: Log,
        layer: FillLayer,
    }

    impl Fake {
        fn boxed(id: u32, opaque: bool, log: &Log) -> Box<dyn Screen> {
            Box::new(Fake {
                id,
                opaque,
                log: log.clone(),
                layer: FillLayer(Color::rgb((id & 0xff) as u8, 0, 0)),
            })
        }
    }

    impl Screen for Fake {
        fn handle(&mut self, input: UiInput) -> ScreenChange {
            self.log.borrow_mut().push((self.id, "handle"));
            match input {
                UiInput::Confirm => ScreenChange::Push(Fake::boxed(self.id * 10, true, &self.log)),
                UiInput::Cancel => ScreenChange::Pop,
                _ => ScreenChange::None,
            }
        }
        fn tick(&mut self) -> ScreenChange {
            self.log.borrow_mut().push((self.id, "tick"));
            ScreenChange::None
        }
        fn is_opaque(&self) -> bool {
            self.opaque
        }
        fn collect_layers<'a>(
            &'a self,
            world: &mut Vec<&'a dyn PixelLayer>,
            _overlays: &mut Vec<&'a dyn OverlayLayer>,
        ) {
            world.push(&self.layer);
        }
    }

    #[test]
    fn input_and_tick_reach_only_the_top() {
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let mut stack = ScreenStack::new(Fake::boxed(1, true, &log));
        stack.handle(UiInput::Confirm); // 1 handles, pushes 10
        assert_eq!(stack.len(), 2);
        log.borrow_mut().clear();
        stack.tick();
        assert_eq!(*log.borrow(), vec![(10, "tick")], "only the top ticks");
    }

    #[test]
    fn push_pop_replace_move_the_stack() {
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let mut stack = ScreenStack::new(Fake::boxed(1, true, &log));
        stack.handle(UiInput::Confirm); // push 10
        assert_eq!(stack.len(), 2);
        stack.handle(UiInput::Cancel); // top pops
        assert_eq!(stack.len(), 1);
        stack.push(Fake::boxed(2, true, &log)); // app-driven push
        stack.pop();
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn rendering_starts_at_the_topmost_opaque_screen() {
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let mut stack = ScreenStack::new(Fake::boxed(1, true, &log));
        stack.push(Fake::boxed(2, false, &log)); // transparent overlay
        stack.push(Fake::boxed(3, true, &log)); // opaque, covers 1 and 2

        // Scoped so the collected layer refs (borrowed from the stack) drop
        // before the mutating `pop` below.
        {
            let mut world: Vec<&dyn PixelLayer> = Vec::new();
            let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
            stack.collect_layers(&mut world, &mut overlays);
            assert_eq!(world.len(), 1, "only the topmost opaque screen renders");
        }

        stack.pop(); // -> [opaque 1, transparent 2]
        {
            let mut world: Vec<&dyn PixelLayer> = Vec::new();
            let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
            stack.collect_layers(&mut world, &mut overlays);
            assert_eq!(world.len(), 2, "opaque base shows through the overlay");
        }
    }
}

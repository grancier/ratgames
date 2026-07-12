//! [`ChallengeScreen`] — the timed challenge controller: pose a prompt, take an
//! answer, run a feedback beat, route onward — with the game supplying only the
//! grading, the content, and the routing.
//!
//! The arcade active-play loop is two phases. *Answering*: editing input routes
//! to the answer widget (a [`ChoiceList`] in multiple-choice mode, the game's
//! shared input field in typed mode), an optional [`TimedGauge`] question clock
//! runs, and [`Confirm`](UiInput::Confirm) commits the answer. *Feedback*: a
//! [`FeedbackBeat`] plays over the frozen challenge — editing is dead, the
//! clock is frozen, [`Confirm`](UiInput::Confirm) skips the wait — and when the
//! beat ends the game's pending route is applied exactly once. The clock's
//! fire-once expiry grades a timed-out miss through the same feedback path, so
//! a timeout sequences identically to a wrong answer.
//!
//! The game's half lives behind the [`Challenge`] driver trait: bake the view
//! for the current problem, grade an answer (or a timeout) into a
//! [`GradedAttempt`], resolve the pending token when the beat ends, and route a
//! cancel. The controller is math-free — it never sees a problem, an answer's
//! meaning, or a score; it owns only the phase machinery and its invariants
//! (freeze, skip, fire-once timeout, resolve-once).
//!
//! The screen reaches the game's shared widgets through the established seams:
//! [`BannerContext`] (the [`ChoiceList`] re-bakes its caret on navigation) and
//! [`InputContext`] (typed answers edit the shared line).

use super::text_entry_screen::InputContext;
use super::{BannerContext, Screen, ScreenChange};
use crate::present::{OverlayLayer, PixelLayer};
use crate::ui::{ChoiceList, FeedbackBeat, FeedbackBeatLayers, ShadowBanner, TimedGauge, UiInput};

/// A committed answer, in whichever mode the view offered: the entered text
/// (typed mode; taken from the shared line, which is cleared for the next
/// entry) or the selected index (multiple-choice mode).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChallengeAnswer {
    /// The submitted text of the shared input line.
    Typed(String),
    /// The selected index of the choice list.
    Choice(usize),
}

/// The answering view for one problem, baked by the game's [`Challenge::view`]:
/// the prompt banner, the status line, the answer widget, and the question
/// clock.
pub struct ChallengeView<Ctx> {
    /// The challenge itself (an equation banner). Shown while answering and
    /// under the feedback beat's opening flash; hidden behind the verdict.
    pub prompt: ShadowBanner,
    /// The status line (a score / lives HUD). Shown in every phase.
    pub status: ShadowBanner,
    /// The multiple-choice list, or `None` for typed mode (the game's shared
    /// input field via [`InputContext`]).
    pub choices: Option<ChoiceList>,
    /// The question clock, or `None` on an untimed problem.
    pub gauge: Option<TimedGauge<Ctx>>,
}

/// A graded attempt (an answer or a timeout): the feedback beat to run, the
/// refreshed status line to show behind it (the score / lives just changed),
/// and the game's opaque routing token, handed back to
/// [`Challenge::resolve`] when the beat ends.
pub struct GradedAttempt<P> {
    /// The beat to play: reject blink or success wash, then the verdict.
    pub beat: FeedbackBeat,
    /// The status line re-baked after grading.
    pub status: ShadowBanner,
    /// What happens once the beat resolves — the game's business.
    pub pending: P,
}

/// What a resolved beat does: stay on the challenge screen (the driver's
/// [`view`](Challenge::view) is re-baked for the next problem) or leave it.
pub enum ChallengeResolution<Ctx> {
    /// Reveal the next problem (or a retry) on this screen.
    Stay,
    /// Route off the screen.
    Leave(ScreenChange<Ctx>),
}

/// The game half of a [`ChallengeScreen`]: content, grading, and routing. The
/// driver is a game-side struct (it carries per-level state — a tally, the
/// level's name), not the shared `Ctx`; every method receives the context.
pub trait Challenge<Ctx> {
    /// The opaque routing token a [`GradedAttempt`] carries from
    /// [`grade`](Self::grade) / [`time_out`](Self::time_out) to
    /// [`resolve`](Self::resolve).
    type Pending;

    /// Bake the answering view for the current problem — called at entry and
    /// after every [`Stay`](ChallengeResolution::Stay) resolution.
    fn view(&mut self, ctx: &Ctx) -> ChallengeView<Ctx>;

    /// Grade a committed answer. `time_left` is the frames still on the
    /// question clock as the answer was committed (`None` on an untimed
    /// problem) — a time bonus reads it.
    fn grade(
        &mut self,
        answer: ChallengeAnswer,
        time_left: Option<u32>,
        ctx: &mut Ctx,
    ) -> GradedAttempt<Self::Pending>;

    /// The question clock ran out: grade the timed-out miss. Fired exactly
    /// once per problem (the clock's expiry is fire-once).
    fn time_out(&mut self, ctx: &mut Ctx) -> GradedAttempt<Self::Pending>;

    /// The feedback beat ended (or was skipped): apply the pending token.
    fn resolve(&mut self, pending: Self::Pending, ctx: &mut Ctx) -> ChallengeResolution<Ctx>;

    /// [`Cancel`](UiInput::Cancel) pressed, in either phase.
    fn cancel(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx>;
}

/// The timed-challenge [`Screen`]: the phase machinery over a game-supplied
/// [`Challenge`] driver. Construct with [`new`](Self::new) and push onto the
/// [`ScreenStack`](crate::ScreenStack).
pub struct ChallengeScreen<Ctx, C: Challenge<Ctx>> {
    driver: C,
    view: ChallengeView<Ctx>,
    /// The running feedback beat and its routing token — the *Feedback* phase.
    /// `None` = the *Answering* phase.
    feedback: Option<(FeedbackBeat, C::Pending)>,
}

impl<Ctx, C: Challenge<Ctx>> ChallengeScreen<Ctx, C> {
    /// A challenge screen over `driver`, its first view baked immediately.
    #[must_use]
    pub fn new(mut driver: C, ctx: &Ctx) -> Self {
        let view = driver.view(ctx);
        Self {
            driver,
            view,
            feedback: None,
        }
    }

    /// Enter the feedback phase for a graded attempt: show the refreshed
    /// status line and run the beat.
    fn begin_feedback(&mut self, graded: GradedAttempt<C::Pending>) {
        self.view.status = graded.status;
        self.feedback = Some((graded.beat, graded.pending));
    }

    /// End the feedback phase and apply its pending route — once; a second
    /// call (a skip racing the beat's own end) finds no phase to resolve.
    fn resolve_feedback(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        let Some((_, pending)) = self.feedback.take() else {
            return ScreenChange::None;
        };
        match self.driver.resolve(pending, ctx) {
            ChallengeResolution::Stay => {
                self.view = self.driver.view(ctx);
                ScreenChange::None
            }
            ChallengeResolution::Leave(change) => change,
        }
    }
}

impl<Ctx: BannerContext + InputContext, C: Challenge<Ctx>> Screen<Ctx> for ChallengeScreen<Ctx, C> {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        // While the beat runs the answer widget is frozen: Confirm skips the
        // wait, Cancel still routes, everything else is ignored.
        if self.feedback.is_some() {
            return match input {
                UiInput::Confirm => self.resolve_feedback(ctx),
                UiInput::Cancel => self.driver.cancel(ctx),
                _ => ScreenChange::None,
            };
        }
        match input {
            UiInput::Confirm => {
                // Commit in whichever mode the view offered; both produce the
                // same graded shape, so the beat is identical.
                let answer = match self.view.choices.as_ref() {
                    Some(choices) => ChallengeAnswer::Choice(choices.selected()),
                    None => {
                        let line = ctx.input_line();
                        let text = line.text().to_string();
                        line.clear();
                        ChallengeAnswer::Typed(text)
                    }
                };
                let time_left = self.view.gauge.as_ref().map(TimedGauge::remaining);
                let graded = self.driver.grade(answer, time_left, ctx);
                self.begin_feedback(graded);
                ScreenChange::None
            }
            UiInput::Cancel => self.driver.cancel(ctx),
            // Everything else navigates the choice list (arrows) or edits the
            // typed line (type / backspace / delete / caret movement).
            other => {
                if let Some(choices) = self.view.choices.as_mut() {
                    let factory = ctx.banner_factory();
                    choices.handle(other, &factory);
                } else {
                    ctx.input_line().handle(other);
                }
                ScreenChange::None
            }
        }
    }

    fn tick(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        // The beat, if running, takes priority and freezes the question clock.
        if self.feedback.is_some() {
            let done = self
                .feedback
                .as_mut()
                .is_some_and(|(beat, _)| beat.advance());
            return if done {
                self.resolve_feedback(ctx)
            } else {
                ScreenChange::None
            };
        }
        // Otherwise run the clock; its fire-once expiry is a timed-out miss,
        // graded through the same feedback path as a wrong answer.
        let timed_out = self
            .view
            .gauge
            .as_mut()
            .is_some_and(|gauge| gauge.advance(ctx));
        if timed_out {
            let graded = self.driver.time_out(ctx);
            self.begin_feedback(graded);
        }
        ScreenChange::None
    }

    fn collect_layers<'a>(
        &'a self,
        ctx: &'a Ctx,
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        match &self.feedback {
            Some((beat, _)) => match beat.layers() {
                // Opening: the reject blink over the frozen challenge.
                FeedbackBeatLayers::Opening { reject } => {
                    overlays.push(&self.view.prompt);
                    overlays.push(&self.view.status);
                    overlays.push(reject);
                }
                // Verdict: the verdict (and a hit's fading wash) over the
                // status line; the prompt is done.
                FeedbackBeatLayers::Verdict { wash, verdict } => {
                    if let Some(wash) = wash {
                        overlays.push(wash);
                    }
                    overlays.push(&self.view.status);
                    overlays.push(verdict);
                }
                // The tick that ends the beat resolves it, so this frame is
                // not normally reached.
                FeedbackBeatLayers::Done => {}
            },
            None => {
                // The clock renders only while answering (frozen and hidden
                // during the beat): the bar beneath, the readout among the
                // overlays.
                if let Some(gauge) = &self.view.gauge {
                    gauge.collect_layers(world, overlays);
                }
                overlays.push(&self.view.prompt);
                overlays.push(&self.view.status);
                match &self.view.choices {
                    Some(choices) => overlays.push(choices),
                    None => overlays.push(ctx.input_overlay()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use super::*;
    use crate::color::Color;
    use crate::geometry::{Point, Rect, Size};
    use crate::glyph::Bitmap8x8;
    use crate::input::InputLine;
    use crate::surface::Surface;
    use crate::ui::{Countdown, MeterBar, ShadowBannerFactory, ShadowStyle};

    struct FakeOverlay;

    impl OverlayLayer for FakeOverlay {
        fn render(&self, _window: &mut Surface, _viewport: Rect) {}
    }

    /// The fake game context: the two seams plus interior-mutable call logs,
    /// so the (screen-owned) driver can record through `&Ctx`.
    struct FakeCtx {
        line: InputLine,
        overlay: FakeOverlay,
        views: Cell<u32>,
        grades: RefCell<Vec<(ChallengeAnswer, Option<u32>)>>,
        timeouts: Cell<u32>,
        resolves: RefCell<Vec<u32>>,
        cancels: Cell<u32>,
    }

    impl FakeCtx {
        fn new() -> Self {
            Self {
                line: InputLine::new(),
                overlay: FakeOverlay,
                views: Cell::new(0),
                grades: RefCell::new(Vec::new()),
                timeouts: Cell::new(0),
                resolves: RefCell::new(Vec::new()),
                cancels: Cell::new(0),
            }
        }
    }

    impl BannerContext for FakeCtx {
        fn banner_factory(&self) -> ShadowBannerFactory<'_> {
            ShadowBannerFactory::new(&Bitmap8x8, ShadowStyle::default(), Size::new(64, 64))
        }
    }

    impl InputContext for FakeCtx {
        fn input_line(&mut self) -> &mut InputLine {
            &mut self.line
        }

        fn input_overlay(&self) -> &dyn OverlayLayer {
            &self.overlay
        }
    }

    /// A scripted driver: configuration only — every call logs into the ctx.
    struct Script {
        with_choices: bool,
        gauge_frames: Option<u32>,
        beat_frames: u32,
        leave_on_resolve: bool,
    }

    impl Script {
        fn typed() -> Self {
            Self {
                with_choices: false,
                gauge_frames: None,
                beat_frames: 2,
                leave_on_resolve: false,
            }
        }

        fn graded(&self, ctx: &FakeCtx, pending: u32) -> GradedAttempt<u32> {
            let factory = ctx.banner_factory();
            GradedAttempt {
                beat: FeedbackBeat::new(
                    None,
                    None,
                    factory.at("V", Point::new(0, 0), 1),
                    Countdown::new(self.beat_frames),
                ),
                status: factory.at("HUD*", Point::new(0, 8), 1),
                pending,
            }
        }
    }

    impl Challenge<FakeCtx> for Script {
        type Pending = u32;

        fn view(&mut self, ctx: &FakeCtx) -> ChallengeView<FakeCtx> {
            ctx.views.set(ctx.views.get() + 1);
            let factory = ctx.banner_factory();
            ChallengeView {
                prompt: factory.at("2+2", Point::new(0, 0), 1),
                status: factory.at("HUD", Point::new(0, 8), 1),
                choices: self.with_choices.then(|| {
                    ChoiceList::new(
                        vec!["3".to_string(), "4".to_string()],
                        Point::new(0, 16),
                        8,
                        1,
                        &factory,
                    )
                }),
                gauge: self.gauge_frames.map(|frames| {
                    TimedGauge::new(
                        Countdown::new(frames),
                        MeterBar::new(
                            Rect::new(Point::new(0, 60), Size::new(64, 2)),
                            Color::rgb(255, 0, 0),
                            Color::rgb(0, 0, 255),
                        ),
                    )
                }),
            }
        }

        fn grade(
            &mut self,
            answer: ChallengeAnswer,
            time_left: Option<u32>,
            ctx: &mut FakeCtx,
        ) -> GradedAttempt<u32> {
            ctx.grades.borrow_mut().push((answer, time_left));
            self.graded(ctx, 7)
        }

        fn time_out(&mut self, ctx: &mut FakeCtx) -> GradedAttempt<u32> {
            ctx.timeouts.set(ctx.timeouts.get() + 1);
            self.graded(ctx, 9)
        }

        fn resolve(&mut self, pending: u32, ctx: &mut FakeCtx) -> ChallengeResolution<FakeCtx> {
            ctx.resolves.borrow_mut().push(pending);
            if self.leave_on_resolve {
                ChallengeResolution::Leave(ScreenChange::Pop)
            } else {
                ChallengeResolution::Stay
            }
        }

        fn cancel(&mut self, ctx: &mut FakeCtx) -> ScreenChange<FakeCtx> {
            ctx.cancels.set(ctx.cancels.get() + 1);
            ScreenChange::None
        }
    }

    #[test]
    fn typed_mode_commits_the_shared_line_and_clears_it() {
        let mut ctx = FakeCtx::new();
        let mut screen = ChallengeScreen::new(Script::typed(), &ctx);
        for c in "42".chars() {
            screen.handle(UiInput::Char(c), &mut ctx);
        }
        screen.handle(UiInput::Confirm, &mut ctx);
        assert_eq!(
            *ctx.grades.borrow(),
            [(ChallengeAnswer::Typed("42".to_string()), None)]
        );
        assert_eq!(ctx.line.text(), "", "cleared for the next answer");
    }

    #[test]
    fn choice_mode_commits_the_selection() {
        let mut ctx = FakeCtx::new();
        let mut screen = ChallengeScreen::new(
            Script {
                with_choices: true,
                ..Script::typed()
            },
            &ctx,
        );
        screen.handle(UiInput::Down, &mut ctx); // navigate to index 1
        screen.handle(UiInput::Confirm, &mut ctx);
        assert_eq!(*ctx.grades.borrow(), [(ChallengeAnswer::Choice(1), None)]);
    }

    #[test]
    fn grading_reads_the_clock_still_left() {
        let mut ctx = FakeCtx::new();
        let mut screen = ChallengeScreen::new(
            Script {
                gauge_frames: Some(10),
                ..Script::typed()
            },
            &ctx,
        );
        screen.tick(&mut ctx); // 1 of 10 spent
        screen.handle(UiInput::Confirm, &mut ctx);
        assert_eq!(
            *ctx.grades.borrow(),
            [(ChallengeAnswer::Typed(String::new()), Some(9))]
        );
    }

    #[test]
    fn the_clock_expiry_grades_a_timeout_exactly_once() {
        let mut ctx = FakeCtx::new();
        let mut screen = ChallengeScreen::new(
            Script {
                gauge_frames: Some(2),
                beat_frames: 10,
                ..Script::typed()
            },
            &ctx,
        );
        screen.tick(&mut ctx); // 1 of 2
        assert_eq!(ctx.timeouts.get(), 0, "still on the clock");
        screen.tick(&mut ctx); // 2 of 2 -> timed out, beat begins
        assert_eq!(ctx.timeouts.get(), 1);
        screen.tick(&mut ctx); // beat frame, clock frozen
        screen.tick(&mut ctx);
        assert_eq!(ctx.timeouts.get(), 1, "the beat froze the clock");
    }

    #[test]
    fn the_beat_freezes_editing_and_resolves_when_done() {
        let mut ctx = FakeCtx::new();
        let mut screen = ChallengeScreen::new(Script::typed(), &ctx); // 2-frame beat
        assert_eq!(ctx.views.get(), 1, "entry bakes the first view");
        screen.handle(UiInput::Confirm, &mut ctx); // grade -> beat
        screen.handle(UiInput::Char('x'), &mut ctx); // frozen
        assert_eq!(ctx.line.text(), "", "editing is dead during the beat");
        screen.tick(&mut ctx); // 1 of 2
        assert!(ctx.resolves.borrow().is_empty(), "beat still holding");
        screen.tick(&mut ctx); // 2 of 2 -> resolve
        assert_eq!(*ctx.resolves.borrow(), [7]);
        assert_eq!(ctx.views.get(), 2, "Stay re-baked the next problem");
    }

    #[test]
    fn confirm_skips_the_beat_and_resolves_once() {
        let mut ctx = FakeCtx::new();
        let mut screen = ChallengeScreen::new(
            Script {
                beat_frames: 100,
                ..Script::typed()
            },
            &ctx,
        );
        screen.handle(UiInput::Confirm, &mut ctx); // grade -> beat
        screen.handle(UiInput::Confirm, &mut ctx); // skip -> resolve
        assert_eq!(*ctx.resolves.borrow(), [7]);
        // The screen is back in the answering phase: a confirm grades again
        // rather than resolving a beat that is no longer running.
        screen.handle(UiInput::Confirm, &mut ctx);
        assert_eq!(ctx.grades.borrow().len(), 2);
        assert_eq!(*ctx.resolves.borrow(), [7], "no double resolve");
    }

    #[test]
    fn a_leave_resolution_returns_the_route() {
        let mut ctx = FakeCtx::new();
        let mut screen = ChallengeScreen::new(
            Script {
                leave_on_resolve: true,
                ..Script::typed()
            },
            &ctx,
        );
        screen.handle(UiInput::Confirm, &mut ctx); // grade -> beat
        let change = screen.handle(UiInput::Confirm, &mut ctx); // skip -> resolve -> leave
        assert!(matches!(change, ScreenChange::Pop));
        assert_eq!(ctx.views.get(), 1, "a leaving resolution bakes no view");
    }

    #[test]
    fn cancel_routes_in_both_phases() {
        let mut ctx = FakeCtx::new();
        let mut screen = ChallengeScreen::new(Script::typed(), &ctx);
        screen.handle(UiInput::Cancel, &mut ctx); // answering phase
        screen.handle(UiInput::Confirm, &mut ctx); // -> beat
        screen.handle(UiInput::Cancel, &mut ctx); // feedback phase
        assert_eq!(ctx.cancels.get(), 2);
    }

    #[test]
    fn collect_layers_swaps_the_answer_widget_for_the_beat() {
        let mut ctx = FakeCtx::new();
        let mut screen = ChallengeScreen::new(
            Script {
                gauge_frames: Some(10),
                ..Script::typed()
            },
            &ctx,
        );
        {
            let mut world: Vec<&dyn PixelLayer> = Vec::new();
            let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
            screen.collect_layers(&ctx, &mut world, &mut overlays);
            assert_eq!(world.len(), 1, "the clock's bar while answering");
            // prompt + status + the typed field's overlay.
            assert_eq!(overlays.len(), 3);
        }
        screen.handle(UiInput::Confirm, &mut ctx); // -> beat (verdict-only)
        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        screen.collect_layers(&ctx, &mut world, &mut overlays);
        assert!(world.is_empty(), "the clock hides during the beat");
        // status + verdict (no reject, no wash in this script).
        assert_eq!(overlays.len(), 2);
    }
}

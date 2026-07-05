//! The windowed shell: the shared screen context and the five screens
//! (title → name entry → play → result → high scores) driven on a `ScreenStack`.
//!
//! The durable run state lives in [`MathgameSession`]; a screen holds only local
//! UI state (its cached banners). Input mutates the context (`&mut Ctx`);
//! rendering reads it (`&Ctx`). The pixel-art text style and the virtual screen
//! size come from the config, threaded through the context, not constants.
//!
//! Every text element is a [`ShadowBanner`] — an [`OverlayLayer`] that draws crisp
//! integer-scaled 8-bit glyphs with a real device-space drop shadow. The app
//! therefore pushes nothing to the pixel `world`; the banners composite over the
//! upscaled backdrop, anchored to the game viewport so they track the window and
//! letterbox exactly as the old pixel layers did.

use mathgame_app::{AttemptReport, MathgameSession};
use ratgames::{
    BannerAnchor, BigText, Blink, Color, Flash, GlyphSource, HighScoreLayout, HighScores,
    InputField, JsonHighScoreStore, Menu, OverlayLayer, PixelLayer, Point, RunPhase, Screen,
    ScreenChange, ShadowBanner, Size, UiInput,
};

use crate::config::{FeedbackConfig, TextStyle};
use crate::scores;

/// The context threaded through the screen stack: the durable run state, the one
/// shared answer field (it owns a system font, so it lives here rather than per
/// screen), the pixel-art text style, the virtual screen size (for the banners to
/// recover the fit factor), and a quit flag the host loop watches.
pub struct Ctx {
    pub session: MathgameSession,
    pub input: InputField,
    pub text: TextStyle,
    /// The glyph source the pixel-art banners render through (a 32px Menlo raster
    /// in the shipped config), resolved once and shared.
    pub glyphs: Box<dyn GlyphSource>,
    pub feedback: FeedbackConfig,
    pub virtual_size: Size,
    /// The in-memory board, persisted through `store` as runs place.
    pub scores: HighScores,
    /// The persistence seam for `scores`, bound to the config path at startup.
    pub store: JsonHighScoreStore,
    /// The board's "top N" cap, applied when recording (a board never stores it).
    pub capacity: usize,
    pub quit: bool,
}

impl Ctx {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        session: MathgameSession,
        input: InputField,
        text: TextStyle,
        glyphs: Box<dyn GlyphSource>,
        feedback: FeedbackConfig,
        virtual_size: Size,
        scores: HighScores,
        store: JsonHighScoreStore,
        capacity: usize,
    ) -> Self {
        Self {
            session,
            input,
            text,
            glyphs,
            feedback,
            virtual_size,
            scores,
            store,
            capacity,
            quit: false,
        }
    }

    /// Record the finished run on the board and persist it — called once as a run
    /// ends, before the results and high-score screens read the board.
    fn record_run(&mut self) {
        let name = self.session.profile().name().to_string();
        let points = self.session.run().score().points();
        scores::record_and_save(&self.store, &mut self.scores, &name, points, self.capacity);
    }
}

/// Bake `text` into a `ratgames::ShadowBanner` in the app's pixel-art style:
/// `source`'s glyphs (a 32px Menlo raster in the shipped config), magnified by
/// `scale_mult × fit`, with the config's em-relative drop shadow. The reusable
/// render mechanic lives in `ratgames`; this only maps the app's [`TextStyle`]
/// onto it.
fn shadow_banner(
    text: &str,
    source: &dyn GlyphSource,
    anchor: BannerAnchor,
    scale_mult: u32,
    style: TextStyle,
    virtual_size: Size,
) -> ShadowBanner {
    ShadowBanner::new(
        text,
        &BigText::new(1),
        source,
        style.shadow.style(),
        anchor,
        virtual_size,
    )
    .scale(scale_mult)
}

/// A centred banner at the banner scale.
fn banner(
    text: &str,
    source: &dyn GlyphSource,
    style: TextStyle,
    virtual_size: Size,
) -> ShadowBanner {
    shadow_banner(
        text,
        source,
        BannerAnchor::Center,
        style.banner_scale,
        style,
        virtual_size,
    )
}

/// A banner anchored at a virtual-screen point, at `scale_mult`.
fn banner_at(
    text: &str,
    source: &dyn GlyphSource,
    at: Point,
    scale_mult: u32,
    style: TextStyle,
    virtual_size: Size,
) -> ShadowBanner {
    shadow_banner(
        text,
        source,
        BannerAnchor::Virtual(at),
        scale_mult,
        style,
        virtual_size,
    )
}

/// The top-of-screen score / lives / level line, anchored top-left.
fn hud(
    session: &MathgameSession,
    source: &dyn GlyphSource,
    style: TextStyle,
    virtual_size: Size,
) -> ShadowBanner {
    let run = session.run();
    let text = format!(
        "SCORE {}  LIVES {}  L{}",
        run.score().points(),
        run.lives().count(),
        run.levels().current() + 1,
    );
    banner_at(
        &text,
        source,
        Point::new(4, 4),
        style.hud_scale,
        style,
        virtual_size,
    )
}

/// Title screen: a banner. Enter starts, Esc quits.
pub struct TitleScreen {
    banner: ShadowBanner,
}

impl TitleScreen {
    #[must_use]
    pub fn new(source: &dyn GlyphSource, style: TextStyle, virtual_size: Size) -> Self {
        Self {
            banner: banner("MATH GAME", source, style, virtual_size),
        }
    }
}

impl Screen<Ctx> for TitleScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Confirm => {
                ctx.input.set_prompt("NAME: ");
                ScreenChange::Replace(Box::new(NameEntryScreen))
            }
            UiInput::Cancel => {
                ctx.quit = true;
                ScreenChange::None
            }
            _ => ScreenChange::None,
        }
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        _world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        overlays.push(&self.banner);
    }
}

/// Name entry: type into the shared answer field; Enter records the name and
/// starts play.
struct NameEntryScreen;

impl Screen<Ctx> for NameEntryScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Confirm => {
                let name = ctx.input.submit();
                let name = if name.trim().is_empty() {
                    "PLAYER".to_string()
                } else {
                    name
                };
                ctx.session.set_player_name(name);
                ctx.input.set_prompt("ANSWER: ");
                ScreenChange::Replace(Box::new(PlayScreen::new(
                    &ctx.session,
                    &*ctx.glyphs,
                    ctx.text,
                    ctx.virtual_size,
                )))
            }
            UiInput::Cancel => {
                ctx.quit = true;
                ScreenChange::None
            }
            // Every other event is line editing (type, backspace, forward-delete,
            // caret movement); the field ignores the ones it does not own.
            other => {
                ctx.input.handle(other);
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
        overlays.push(&ctx.input);
    }
}

/// What to do when the feedback beat ends: reveal the next problem, or leave for
/// the result screen because the run finished on this answer.
#[derive(Debug, PartialEq, Eq)]
enum Pending {
    Advance,
    Finish(RunPhase),
}

/// The verdict line for a graded answer — the clarity-critical text. A hit reads
/// `CORRECT`; a miss states the correct answer plainly (`ANSWER IS 7`, never
/// `WRONG 7`, which reads as if 7 were the wrong answer). Pure and font-free, so
/// it is unit-tested directly.
fn verdict_line(report: &AttemptReport) -> String {
    if report.correct {
        "CORRECT".to_string()
    } else {
        match report.evaluation.as_ref() {
            Some(evaluation) => {
                format!(
                    "ANSWER IS {}",
                    evaluation.canonical_answer().to_fraction_string()
                )
            }
            None => "WRONG".to_string(),
        }
    }
}

/// What the beat does when it ends: reveal the next problem, or hand off to the
/// result screen because the run finished on this answer.
fn pending_for(report: &AttemptReport) -> Pending {
    if report.run_phase == RunPhase::Playing {
        Pending::Advance
    } else {
        Pending::Finish(report.run_phase)
    }
}

/// Bake the flashing red reject cross: the same "X" glyph as the banner letters
/// (from `source`), as a tight red sprite that blinks `flashes` times at
/// `cross_scale`. `GlyphMask::to_sprite` crops to the glyph's ink so the lone "X"
/// centres cleanly (a `BigText` bake would pad and blob it).
fn reject_cross(cfg: &FeedbackConfig, source: &dyn GlyphSource, virtual_size: Size) -> Blink {
    let cross = source.glyph('X').to_sprite(cfg.wrong_color);
    Blink::new(cross, BannerAnchor::Center, virtual_size)
        .scale(cfg.cross_scale)
        .pattern(cfg.flashes, cfg.flash_frames, cfg.flash_frames)
}

/// A success wash and the full-strength colour it fades from.
struct Wash {
    flash: Flash,
    base: Color,
}

/// The per-answer feedback beat. A miss opens with a flashing red reject cross
/// (`cross`) over the frozen problem; then both hit and miss show a centred
/// verdict banner for `duration` frames, a hit additionally tinting the screen
/// with a success `wash` that fades out. `pending` is applied when the verdict
/// elapses. The answer field is frozen throughout.
struct Feedback {
    cross: Option<Blink>,
    wash: Option<Wash>,
    verdict: ShadowBanner,
    remaining: u32,
    duration: u32,
    pending: Pending,
}

/// Multiple-choice answer state: ratgames' pure [`Menu`] selection model plus the
/// baked choice banners, re-baked when the highlight moves or the problem changes.
/// Present only when the session is in multiple-choice mode; typed play uses the
/// shared answer field instead.
struct Choices {
    menu: Menu,
    banners: Vec<ShadowBanner>,
}

impl Choices {
    /// Build from the session's current choices, or `None` in typed mode.
    fn new(
        session: &MathgameSession,
        source: &dyn GlyphSource,
        style: TextStyle,
        virtual_size: Size,
    ) -> Option<Self> {
        let labels = session.current_choices()?;
        let banners = choice_banners(&labels, 0, source, style, virtual_size);
        Some(Self {
            menu: Menu::new(labels),
            banners,
        })
    }

    /// Re-bake the banners to mark the current highlight.
    fn rehighlight(&mut self, source: &dyn GlyphSource, style: TextStyle, virtual_size: Size) {
        let labels: Vec<String> = self.menu.items().to_vec();
        self.banners = choice_banners(&labels, self.menu.selected(), source, style, virtual_size);
    }
}

/// Bake the choice list as a left-anchored vertical stack of pixel-art banners,
/// the selected one marked with a leading caret (a marker rather than a colour, so
/// it reads on the 8-bit palette). Layout constants stay app-side, like the board's.
fn choice_banners(
    labels: &[String],
    selected: usize,
    source: &dyn GlyphSource,
    style: TextStyle,
    virtual_size: Size,
) -> Vec<ShadowBanner> {
    const CHOICES_X: i32 = 40;
    const CHOICES_Y: i32 = 150;
    const ROW_PITCH: i32 = 46;
    labels
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let text = if i == selected {
                format!("> {label}")
            } else {
                format!("  {label}")
            };
            banner_at(
                &text,
                source,
                Point::new(CHOICES_X, CHOICES_Y + i as i32 * ROW_PITCH),
                style.hud_scale,
                style,
                virtual_size,
            )
        })
        .collect()
}

/// The equation banner, placed for the answer mode: centred (above the bottom
/// input field) in typed mode, or anchored near the top — clear of the choice
/// list below it — in multiple-choice mode.
fn equation_banner(
    session: &MathgameSession,
    source: &dyn GlyphSource,
    style: TextStyle,
    virtual_size: Size,
) -> ShadowBanner {
    let prompt = session.current_prompt();
    if session.current_choices().is_some() {
        banner_at(
            &prompt,
            source,
            Point::new(40, 40),
            style.banner_scale,
            style,
            virtual_size,
        )
    } else {
        banner(&prompt, source, style, virtual_size)
    }
}

/// Play: the current equation as a banner, a score/lives HUD, and the answer —
/// either the shared typed field or a multiple-choice list. Enter grades the
/// answer, then a brief feedback beat flashes the verdict (and the correct answer
/// on a miss) before the next problem or the result screen.
struct PlayScreen {
    equation: ShadowBanner,
    hud: ShadowBanner,
    style: TextStyle,
    virtual_size: Size,
    feedback: Option<Feedback>,
    /// The multiple-choice selection, or `None` when the session is typed.
    choices: Option<Choices>,
}

impl PlayScreen {
    fn new(
        session: &MathgameSession,
        source: &dyn GlyphSource,
        style: TextStyle,
        virtual_size: Size,
    ) -> Self {
        Self {
            equation: equation_banner(session, source, style, virtual_size),
            hud: hud(session, source, style, virtual_size),
            style,
            virtual_size,
            feedback: None,
            choices: Choices::new(session, source, style, virtual_size),
        }
    }

    fn refresh(&mut self, source: &dyn GlyphSource, session: &MathgameSession) {
        self.equation = equation_banner(session, source, self.style, self.virtual_size);
        self.hud = hud(session, source, self.style, self.virtual_size);
        self.choices = Choices::new(session, source, self.style, self.virtual_size);
    }

    /// Open the feedback beat for a graded answer: refresh the HUD so the new
    /// score / lives show behind it, arm a miss's flashing cross or a hit's
    /// success wash, bake the verdict, and record what to do when the beat ends.
    fn begin_feedback(&mut self, ctx: &Ctx, report: &AttemptReport) {
        let cfg = ctx.feedback;
        let source = &*ctx.glyphs;
        self.hud = hud(&ctx.session, source, self.style, self.virtual_size);
        let (cross, wash) = if report.correct {
            let wash = Wash {
                flash: Flash::new(cfg.correct_color),
                base: cfg.correct_color,
            };
            (None, Some(wash))
        } else {
            (Some(reject_cross(&cfg, source, self.virtual_size)), None)
        };
        self.feedback = Some(Feedback {
            cross,
            wash,
            verdict: banner(&verdict_line(report), source, self.style, self.virtual_size),
            remaining: cfg.duration_frames,
            duration: cfg.duration_frames,
            pending: pending_for(report),
        });
    }

    /// End the feedback beat and apply its pending action: reveal the next
    /// problem, or hand off to the result screen.
    fn resolve_feedback(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        let Some(feedback) = self.feedback.take() else {
            return ScreenChange::None;
        };
        match feedback.pending {
            Pending::Advance => {
                self.refresh(&*ctx.glyphs, &ctx.session);
                ScreenChange::None
            }
            Pending::Finish(phase) => {
                ctx.record_run();
                ScreenChange::Replace(Box::new(ResultScreen::new(
                    &ctx.session,
                    &*ctx.glyphs,
                    phase,
                    ctx.text,
                    ctx.virtual_size,
                )))
            }
        }
    }
}

impl Screen<Ctx> for PlayScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        // While the feedback beat runs the answer field is frozen: Enter skips
        // the wait, Esc still quits, everything else is ignored.
        if self.feedback.is_some() {
            return match input {
                UiInput::Confirm => self.resolve_feedback(ctx),
                UiInput::Cancel => {
                    ctx.quit = true;
                    ScreenChange::None
                }
                _ => ScreenChange::None,
            };
        }
        match input {
            UiInput::Confirm => {
                // Grade the picked choice in multiple-choice mode, else the typed
                // answer. Both produce the same report, so the beat is identical.
                let report = if let Some(choices) = self.choices.as_ref() {
                    ctx.session.submit_choice(choices.menu.selected())
                } else {
                    let answer = ctx.input.submit();
                    ctx.session.submit_typed_answer(answer)
                };
                self.begin_feedback(ctx, &report);
                ScreenChange::None
            }
            UiInput::Cancel => {
                ctx.quit = true;
                ScreenChange::None
            }
            // Everything else navigates the choices (arrows/Confirm via the menu)
            // or edits the typed line (type/backspace/delete/caret movement).
            other => {
                let (style, virtual_size) = (self.style, self.virtual_size);
                if let Some(choices) = self.choices.as_mut() {
                    choices.menu.handle(other);
                    choices.rehighlight(&*ctx.glyphs, style, virtual_size);
                } else {
                    ctx.input.handle(other);
                }
                ScreenChange::None
            }
        }
    }

    fn tick(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        let resolve = match self.feedback.as_mut() {
            None => return ScreenChange::None,
            Some(feedback) => match feedback.cross.as_mut() {
                // Phase 1 (a miss): pump the flashing cross; when it finishes,
                // drop it so the verdict phase begins next frame.
                Some(cross) => {
                    cross.advance();
                    if cross.is_done() {
                        feedback.cross = None;
                    }
                    false
                }
                // Phase 2: fade a hit's wash, count the verdict down, then resolve.
                None => {
                    if let Some(wash) = feedback.wash.as_mut() {
                        wash.flash.set_color(
                            wash.base.scale_alpha(feedback.remaining, feedback.duration),
                        );
                    }
                    if feedback.remaining > 0 {
                        feedback.remaining -= 1;
                    }
                    feedback.remaining == 0
                }
            },
        };
        if resolve {
            self.resolve_feedback(ctx)
        } else {
            ScreenChange::None
        }
    }

    fn collect_layers<'a>(
        &'a self,
        ctx: &'a Ctx,
        _world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        match &self.feedback {
            Some(feedback) => match &feedback.cross {
                // Phase 1: the red X blinks over the frozen problem.
                Some(cross) => {
                    overlays.push(&self.equation);
                    overlays.push(&self.hud);
                    overlays.push(cross);
                }
                // Phase 2: the verdict (and a hit's fading wash) over the HUD.
                None => {
                    if let Some(wash) = &feedback.wash {
                        overlays.push(&wash.flash);
                    }
                    overlays.push(&self.hud);
                    overlays.push(&feedback.verdict);
                }
            },
            None => {
                overlays.push(&self.equation);
                overlays.push(&self.hud);
                match &self.choices {
                    Some(choices) => {
                        for banner in &choices.banners {
                            overlays.push(banner);
                        }
                    }
                    None => overlays.push(&ctx.input),
                }
            }
        }
    }
}

/// Result: a win / game-over banner and the final score. Enter shows the board.
struct ResultScreen {
    banner: ShadowBanner,
    score: ShadowBanner,
}

impl ResultScreen {
    fn new(
        session: &MathgameSession,
        source: &dyn GlyphSource,
        phase: RunPhase,
        style: TextStyle,
        virtual_size: Size,
    ) -> Self {
        let title = if phase == RunPhase::Won {
            "YOU WIN"
        } else {
            "GAME OVER"
        };
        let score = format!("SCORE {}   ENTER", session.run().score().points());
        Self {
            banner: banner(title, source, style, virtual_size),
            score: banner_at(
                &score,
                source,
                Point::new(4, 4),
                style.hud_scale,
                style,
                virtual_size,
            ),
        }
    }
}

impl Screen<Ctx> for ResultScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Confirm => ScreenChange::Replace(Box::new(HighScoreScreen::new(
                &ctx.scores,
                &*ctx.glyphs,
                ctx.text,
                ctx.capacity,
                ctx.virtual_size,
            ))),
            UiInput::Cancel => {
                ctx.quit = true;
                ScreenChange::None
            }
            _ => ScreenChange::None,
        }
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        _world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        overlays.push(&self.banner);
        overlays.push(&self.score);
    }
}

/// High scores: the ranked board shown after a run ends. Enter resets and returns
/// to the title; Esc quits.
struct HighScoreScreen {
    lines: Vec<ShadowBanner>,
}

impl HighScoreScreen {
    /// A header, the board entries (up to `capacity`) in two columns, and a footer
    /// hint — all banners in the config text style, anchored to virtual-screen
    /// positions. Two columns because at 32px a ten-row board is far taller than
    /// the 360px screen; five per column fits comfortably.
    fn new(
        scores: &HighScores,
        source: &dyn GlyphSource,
        style: TextStyle,
        capacity: usize,
        virtual_size: Size,
    ) -> Self {
        const MARGIN_X: i32 = 16;
        const HEADER_Y: i32 = 8;
        const FOOTER_GAP: i32 = 12;

        // ratgames formats and grid-places the ranked rows; the app renders each as
        // a ShadowBanner in its own style and adds the header / footer copy.
        let layout = HighScoreLayout {
            origin: Point::new(MARGIN_X, 60),
            row_pitch: 36,
            column_width: 300,
            rows_per_column: 5,
            name_width: 5,
        };

        let mut lines = vec![banner_at(
            "HIGH SCORES",
            source,
            Point::new(MARGIN_X, HEADER_Y),
            style.banner_scale,
            style,
            virtual_size,
        )];

        for row in layout.rows(scores, capacity) {
            lines.push(banner_at(
                &row.text,
                source,
                row.at,
                style.hud_scale,
                style,
                virtual_size,
            ));
        }

        let footer = layout.below(scores, capacity);
        lines.push(banner_at(
            "PRESS ENTER",
            source,
            Point::new(footer.x, footer.y + FOOTER_GAP),
            style.hud_scale,
            style,
            virtual_size,
        ));

        Self { lines }
    }
}

impl Screen<Ctx> for HighScoreScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Confirm => {
                ctx.session.reset();
                ScreenChange::Replace(Box::new(TitleScreen::new(
                    &*ctx.glyphs,
                    ctx.text,
                    ctx.virtual_size,
                )))
            }
            UiInput::Cancel => {
                ctx.quit = true;
                ScreenChange::None
            }
            _ => ScreenChange::None,
        }
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        _world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        for line in &self.lines {
            overlays.push(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mathgame_core::{DirectArithmetic, Generator, Operator, Response, Rng, evaluate};
    use ratgames::{Bitmap8x8, LevelOutcome};

    fn cfg() -> FeedbackConfig {
        FeedbackConfig {
            correct_color: Color::argb(0x99, 0x39, 0xD3, 0x53),
            wrong_color: Color::rgb(0xE0, 0x2C, 0x2C),
            duration_frames: 30,
            cross_scale: 8,
            flashes: 3,
            flash_frames: 12,
        }
    }

    fn report(
        correct: bool,
        run_phase: RunPhase,
        evaluation: Option<mathgame_core::Evaluation>,
    ) -> AttemptReport {
        AttemptReport {
            correct,
            level_outcome: LevelOutcome::InProgress,
            run_phase,
            evaluation,
        }
    }

    #[test]
    fn a_correct_answer_reads_correct_and_advances() {
        assert_eq!(
            verdict_line(&report(true, RunPhase::Playing, None)),
            "CORRECT"
        );
        assert_eq!(
            pending_for(&report(true, RunPhase::Playing, None)),
            Pending::Advance
        );
    }

    #[test]
    fn a_wrong_answer_states_the_correct_answer_without_ambiguity() {
        // A real evaluation of a wrong typed answer carries the canonical answer;
        // the verdict must state it as THE ANSWER, not beside WRONG (which reads as
        // if that number were the wrong answer) — the whole point of this rework.
        let generator = DirectArithmetic::new("t", "addition", Operator::Add, 0..=9).unwrap();
        let mut rng = Rng::new(1);
        let problem = generator.generate(&mut rng);
        let expected = problem.canonical_solution().to_fraction_string();
        let evaluation = evaluate(&problem, &Response::Typed("999".into()));
        assert!(!evaluation.is_correct());

        let line = verdict_line(&report(false, RunPhase::Playing, Some(evaluation)));
        assert_eq!(line, format!("ANSWER IS {expected}"));
    }

    #[test]
    fn a_missing_evaluation_falls_back_to_a_bare_wrong() {
        assert_eq!(
            verdict_line(&report(false, RunPhase::Playing, None)),
            "WRONG"
        );
    }

    #[test]
    fn a_finished_run_hands_off_to_the_result_screen() {
        // A won run ends on a correct answer, a game-over on a wrong one; either
        // way the beat hands off to the result screen instead of advancing.
        for phase in [RunPhase::Won, RunPhase::GameOver] {
            let correct = phase == RunPhase::Won;
            assert_eq!(
                pending_for(&report(correct, phase, None)),
                Pending::Finish(phase)
            );
        }
    }

    #[test]
    fn the_reject_cross_blinks_the_configured_number_of_times() {
        let cfg = cfg();
        // flashes × (on + off), each phase flash_frames long.
        let total = cfg.flashes * cfg.flash_frames * 2;
        let mut cross = reject_cross(&cfg, &Bitmap8x8, Size::new(256, 256));
        for _ in 0..total - 1 {
            cross.advance();
            assert!(!cross.is_done());
        }
        cross.advance();
        assert!(cross.is_done());
    }
}

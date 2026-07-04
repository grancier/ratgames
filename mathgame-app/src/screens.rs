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
    BannerAnchor, BigText, Bitmap8x8, Blink, Color, Flash, GlyphSource, HighScores, InputField,
    OverlayLayer, PixelLayer, Point, RunPhase, Screen, ScreenChange, ShadowBanner, Size, Sprite,
    UiInput,
};

use crate::config::{FeedbackConfig, ScoresConfig, TextStyle};
use crate::scores;

/// The context threaded through the screen stack: the durable run state, the one
/// shared answer field (it owns a system font, so it lives here rather than per
/// screen), the pixel-art text style, the virtual screen size (for the banners to
/// recover the fit factor), and a quit flag the host loop watches.
pub struct Ctx {
    pub session: MathgameSession,
    pub input: InputField,
    pub text: TextStyle,
    pub feedback: FeedbackConfig,
    pub virtual_size: Size,
    pub scores: HighScores,
    pub scores_cfg: ScoresConfig,
    pub quit: bool,
}

impl Ctx {
    pub fn new(
        session: MathgameSession,
        input: InputField,
        text: TextStyle,
        feedback: FeedbackConfig,
        virtual_size: Size,
        scores: HighScores,
        scores_cfg: ScoresConfig,
    ) -> Self {
        Self {
            session,
            input,
            text,
            feedback,
            virtual_size,
            scores,
            scores_cfg,
            quit: false,
        }
    }

    /// Record the finished run on the board and persist it — called once as a run
    /// ends, before the results and high-score screens read the board.
    fn record_run(&mut self) {
        let name = self.session.profile().name().to_string();
        let points = self.session.run().score().points();
        scores::record_and_save(
            &mut self.scores,
            &name,
            points,
            self.scores_cfg.capacity,
            &self.scores_cfg.file,
        );
    }
}

/// Bake `text` into a `ratgames::ShadowBanner` in the app's pixel-art style:
/// chunky 8x8 glyphs, magnified by `scale_mult × fit`, with the config's
/// em-relative drop shadow. The reusable render mechanic lives in `ratgames`;
/// this only maps the app's [`TextStyle`] onto it.
fn shadow_banner(
    text: &str,
    anchor: BannerAnchor,
    scale_mult: u32,
    style: TextStyle,
    virtual_size: Size,
) -> ShadowBanner {
    ShadowBanner::new(
        text,
        &BigText::new(1),
        &Bitmap8x8,
        style.shadow.style(),
        anchor,
        virtual_size,
    )
    .scale(scale_mult)
}

/// A centred banner at the banner scale.
fn banner(text: &str, style: TextStyle, virtual_size: Size) -> ShadowBanner {
    shadow_banner(
        text,
        BannerAnchor::Center,
        style.banner_scale,
        style,
        virtual_size,
    )
}

/// A banner anchored at a virtual-screen point, at `scale_mult`.
fn banner_at(
    text: &str,
    at: Point,
    scale_mult: u32,
    style: TextStyle,
    virtual_size: Size,
) -> ShadowBanner {
    shadow_banner(
        text,
        BannerAnchor::Virtual(at),
        scale_mult,
        style,
        virtual_size,
    )
}

/// The top-of-screen score / lives / level line, anchored top-left.
fn hud(session: &MathgameSession, style: TextStyle, virtual_size: Size) -> ShadowBanner {
    let run = session.run();
    let text = format!(
        "SCORE {}  LIVES {}  L{}",
        run.score().points(),
        run.lives().count(),
        run.levels().current() + 1,
    );
    banner_at(
        &text,
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
    pub fn new(style: TextStyle, virtual_size: Size) -> Self {
        Self {
            banner: banner("MATH GAME", style, virtual_size),
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
            UiInput::Char(c) => {
                ctx.input.type_char(c);
                ScreenChange::None
            }
            UiInput::Backspace => {
                ctx.input.backspace();
                ScreenChange::None
            }
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
        ctx: &'a Ctx,
        _world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        overlays.push(&ctx.input);
    }
}

/// The wash colour at `remaining/duration` of its configured strength: a linear
/// fade of the alpha channel that holds the RGB, so the flash decays to nothing
/// over the beat. `duration` is treated as at least 1.
fn faded(color: Color, remaining: u32, duration: u32) -> Color {
    let rgb = color.packed();
    let alpha = (u32::from(color.alpha()) * remaining / duration.max(1)) as u8;
    Color::argb(alpha, (rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
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

/// Bake the flashing red reject cross: the same 8x8 "X" glyph as the banner
/// letters, as a tight red sprite that blinks `flashes` times at `cross_scale`.
fn reject_cross(cfg: &FeedbackConfig, virtual_size: Size) -> Blink {
    let cross = glyph_sprite(&Bitmap8x8, 'X', cfg.wrong_color);
    Blink::new(cross, BannerAnchor::Center, virtual_size)
        .scale(cfg.cross_scale)
        .pattern(cfg.flashes, cfg.flash_frames, cfg.flash_frames)
}

/// Bake a single glyph into a tight [`Sprite`] in `ink` — cropped to the glyph's
/// ink bounds, so it centres on the character. A `BigText` bake carries layout
/// padding that shifts a lone glyph well off-centre, and a silhouette of its
/// fill + outline + drop shadow reads as a blob; going straight to the glyph mask
/// avoids both.
fn glyph_sprite(source: &dyn GlyphSource, ch: char, ink: Color) -> Sprite {
    let mask = source.glyph(ch);
    let (mut x0, mut y0, mut x1, mut y1) = (mask.width, mask.height, 0, 0);
    let mut any = false;
    for y in 0..mask.height {
        for x in 0..mask.width {
            if mask.get(x, y) {
                any = true;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if !any {
        return Sprite::new(Size::new(1, 1));
    }
    let mut sprite = Sprite::new(Size::new(x1 - x0 + 1, y1 - y0 + 1));
    for y in y0..=y1 {
        for x in x0..=x1 {
            if mask.get(x, y) {
                sprite.set(Point::new((x - x0) as i32, (y - y0) as i32), ink);
            }
        }
    }
    sprite
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

/// Play: the current equation as a banner, a score/lives HUD, and the shared
/// answer field. Enter grades the answer, then a brief feedback beat flashes the
/// verdict (and the correct answer on a miss) before the next problem or the
/// result screen.
struct PlayScreen {
    equation: ShadowBanner,
    hud: ShadowBanner,
    style: TextStyle,
    virtual_size: Size,
    feedback: Option<Feedback>,
}

impl PlayScreen {
    fn new(session: &MathgameSession, style: TextStyle, virtual_size: Size) -> Self {
        Self {
            equation: banner(&session.current_prompt(), style, virtual_size),
            hud: hud(session, style, virtual_size),
            style,
            virtual_size,
            feedback: None,
        }
    }

    fn refresh(&mut self, session: &MathgameSession) {
        self.equation = banner(&session.current_prompt(), self.style, self.virtual_size);
        self.hud = hud(session, self.style, self.virtual_size);
    }

    /// Open the feedback beat for a graded answer: refresh the HUD so the new
    /// score / lives show behind it, arm a miss's flashing cross or a hit's
    /// success wash, bake the verdict, and record what to do when the beat ends.
    fn begin_feedback(&mut self, ctx: &Ctx, report: &AttemptReport) {
        let cfg = ctx.feedback;
        self.hud = hud(&ctx.session, self.style, self.virtual_size);
        let (cross, wash) = if report.correct {
            let wash = Wash {
                flash: Flash::new(cfg.correct_color),
                base: cfg.correct_color,
            };
            (None, Some(wash))
        } else {
            (Some(reject_cross(&cfg, self.virtual_size)), None)
        };
        self.feedback = Some(Feedback {
            cross,
            wash,
            verdict: banner(&verdict_line(report), self.style, self.virtual_size),
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
                self.refresh(&ctx.session);
                ScreenChange::None
            }
            Pending::Finish(phase) => {
                ctx.record_run();
                ScreenChange::Replace(Box::new(ResultScreen::new(
                    &ctx.session,
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
            UiInput::Char(c) => {
                ctx.input.type_char(c);
                ScreenChange::None
            }
            UiInput::Backspace => {
                ctx.input.backspace();
                ScreenChange::None
            }
            UiInput::Confirm => {
                let answer = ctx.input.submit();
                let report = ctx.session.submit_typed_answer(answer);
                self.begin_feedback(ctx, &report);
                ScreenChange::None
            }
            UiInput::Cancel => {
                ctx.quit = true;
                ScreenChange::None
            }
            _ => ScreenChange::None,
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
                        wash.flash.set_color(faded(
                            wash.base,
                            feedback.remaining,
                            feedback.duration,
                        ));
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
                overlays.push(&ctx.input);
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
            banner: banner(title, style, virtual_size),
            score: banner_at(
                &score,
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
                ctx.text,
                ctx.scores_cfg.capacity,
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
    /// A header, one row per board entry (up to `capacity`), and a footer hint —
    /// all banners in the config text style, anchored to virtual-screen positions.
    fn new(scores: &HighScores, style: TextStyle, capacity: usize, virtual_size: Size) -> Self {
        const MARGIN_X: i32 = 8;
        const HEADER_Y: i32 = 4;
        const ROWS_TOP: i32 = 30;
        const ROW_PITCH: i32 = 13;
        const NAME_WIDTH: usize = 8;

        let mut lines = vec![banner_at(
            "HIGH SCORES",
            Point::new(MARGIN_X, HEADER_Y),
            style.banner_scale,
            style,
            virtual_size,
        )];

        for (i, entry) in scores.entries().iter().take(capacity).enumerate() {
            let name: String = entry.name.to_uppercase().chars().take(NAME_WIDTH).collect();
            let text = format!(
                "{:>2} {:<width$}{:>7}",
                i + 1,
                name,
                entry.points,
                width = NAME_WIDTH
            );
            lines.push(banner_at(
                &text,
                Point::new(MARGIN_X, ROWS_TOP + i as i32 * ROW_PITCH),
                style.hud_scale,
                style,
                virtual_size,
            ));
        }

        let shown = scores.entries().len().min(capacity) as i32;
        lines.push(banner_at(
            "PRESS ENTER",
            Point::new(MARGIN_X, ROWS_TOP + shown * ROW_PITCH + 6),
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
                ScreenChange::Replace(Box::new(TitleScreen::new(ctx.text, ctx.virtual_size)))
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
    use ratgames::LevelOutcome;

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
    fn the_reject_cross_is_a_tight_centred_x_glyph() {
        // Straight from the 8x8 mask, cropped to ink: a proper X, not a padded,
        // off-centre, silhouetted blob.
        let x = glyph_sprite(&Bitmap8x8, 'X', Color::rgb(0xE0, 0x2C, 0x2C));
        assert_eq!(x.size(), Size::new(7, 7)); // trimmed to the X's ink bounds
        let red = Color::rgb(0xE0, 0x2C, 0x2C);
        assert_eq!(x.get(Point::new(0, 0)), red); // top-left arm
        assert_eq!(x.get(Point::new(6, 6)), red); // bottom-right arm
        assert_eq!(x.get(Point::new(3, 3)), red); // the crossing
        assert!(!x.get(Point::new(3, 0)).is_visible()); // the gap between the top arms
    }

    #[test]
    fn the_reject_cross_blinks_the_configured_number_of_times() {
        let cfg = cfg();
        // flashes × (on + off), each phase flash_frames long.
        let total = cfg.flashes * cfg.flash_frames * 2;
        let mut cross = reject_cross(&cfg, Size::new(256, 256));
        for _ in 0..total - 1 {
            cross.advance();
            assert!(!cross.is_done());
        }
        cross.advance();
        assert!(cross.is_done());
    }

    #[test]
    fn the_wash_fades_its_alpha_linearly_and_holds_the_rgb() {
        let base = Color::argb(0x80, 0x12, 0x34, 0x56);
        assert_eq!(faded(base, 10, 10), base); // full strength at the start
        assert_eq!(faded(base, 0, 10).alpha(), 0); // gone at the end
        assert_eq!(faded(base, 5, 10).alpha(), 0x40); // ~half in the middle (0x80*5/10)
        assert_eq!(faded(base, 5, 10).packed(), 0x0012_3456); // rgb preserved through the fade
    }
}

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
    BannerAnchor, BigText, Bitmap8x8, Color, Flash, HighScores, InputField, OverlayLayer,
    PixelLayer, Point, RunPhase, Screen, ScreenChange, ShadowBanner, Size, UiInput,
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

/// The wash colour, verdict text, and post-beat action for a graded answer — the
/// pure decision behind the feedback beat, kept font-free so it is unit-tested
/// without a system font. On a miss the verdict carries the canonical answer.
fn feedback_plan(report: &AttemptReport, cfg: FeedbackConfig) -> (Color, String, Pending) {
    let pending = if report.run_phase == RunPhase::Playing {
        Pending::Advance
    } else {
        Pending::Finish(report.run_phase)
    };
    if report.correct {
        (cfg.correct_color, "CORRECT".to_string(), pending)
    } else {
        let verdict = match report.evaluation.as_ref() {
            Some(evaluation) => {
                format!(
                    "WRONG {}",
                    evaluation.canonical_answer().to_fraction_string()
                )
            }
            None => "WRONG".to_string(),
        };
        (cfg.wrong_color, verdict, pending)
    }
}

/// The per-answer feedback beat: a translucent [`Flash`] wash (faded out over the
/// beat) behind a centred verdict banner, held for `duration` frames before
/// `pending` is applied. Built from an [`AttemptReport`]; the answer field is
/// frozen while it runs.
struct Feedback {
    flash: Flash,
    base_color: Color,
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

    /// Open the feedback beat for a graded answer: pick the wash colour and
    /// verdict from `report`, refresh the HUD so the new score / lives show behind
    /// the wash, and record what to do when the beat ends.
    fn begin_feedback(&mut self, ctx: &Ctx, report: &AttemptReport) {
        let cfg = ctx.feedback;
        let (base_color, verdict_text, pending) = feedback_plan(report, cfg);
        self.hud = hud(&ctx.session, self.style, self.virtual_size);
        self.feedback = Some(Feedback {
            flash: Flash::new(base_color),
            base_color,
            verdict: banner(&verdict_text, self.style, self.virtual_size),
            remaining: cfg.duration_frames,
            duration: cfg.duration_frames,
            pending,
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
        let done = match self.feedback.as_mut() {
            None => return ScreenChange::None,
            Some(feedback) => {
                if feedback.remaining > 0 {
                    feedback.remaining -= 1;
                    let color = faded(feedback.base_color, feedback.remaining, feedback.duration);
                    feedback.flash.set_color(color);
                }
                feedback.remaining == 0
            }
        };
        if done {
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
            // The wash sits behind the HUD and verdict, which stay crisp on top.
            Some(feedback) => {
                overlays.push(&feedback.flash);
                overlays.push(&self.hud);
                overlays.push(&feedback.verdict);
            }
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
            wrong_color: Color::argb(0x99, 0xE0, 0x2C, 0x2C),
            duration_frames: 30,
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
    fn a_correct_answer_flashes_the_correct_colour_and_advances() {
        let (color, text, pending) = feedback_plan(&report(true, RunPhase::Playing, None), cfg());
        assert_eq!(color, cfg().correct_color);
        assert_eq!(text, "CORRECT");
        assert_eq!(pending, Pending::Advance);
    }

    #[test]
    fn a_wrong_answer_shows_the_canonical_answer_in_the_verdict() {
        // A real evaluation of a wrong typed answer carries the canonical answer,
        // which the verdict must surface — the point of the miss feedback.
        let generator = DirectArithmetic::new("t", "addition", Operator::Add, 0..=9).unwrap();
        let mut rng = Rng::new(1);
        let problem = generator.generate(&mut rng);
        let expected = problem.canonical_solution().to_fraction_string();
        let evaluation = evaluate(&problem, &Response::Typed("999".into()));
        assert!(!evaluation.is_correct());

        let (color, text, pending) =
            feedback_plan(&report(false, RunPhase::Playing, Some(evaluation)), cfg());
        assert_eq!(color, cfg().wrong_color);
        assert_eq!(text, format!("WRONG {expected}"));
        assert_eq!(pending, Pending::Advance);
    }

    #[test]
    fn a_missing_evaluation_falls_back_to_a_bare_wrong() {
        let (_, text, _) = feedback_plan(&report(false, RunPhase::Playing, None), cfg());
        assert_eq!(text, "WRONG");
    }

    #[test]
    fn a_finished_run_hands_off_to_the_result_screen() {
        // A won run ends on a correct answer, a game-over on a wrong one; either
        // way the beat hands off to the result screen instead of advancing.
        for phase in [RunPhase::Won, RunPhase::GameOver] {
            let correct = phase == RunPhase::Won;
            let (_, _, pending) = feedback_plan(&report(correct, phase, None), cfg());
            assert_eq!(pending, Pending::Finish(phase));
        }
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

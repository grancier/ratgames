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

use mathgame_app::MathgameSession;
use ratgames::{
    HighScores, InputField, OverlayLayer, PixelLayer, Point, RunPhase, Screen, ScreenChange, Size,
    UiInput,
};

use crate::config::{ScoresConfig, TextStyle};
use crate::scores;
use crate::shadow_banner::ShadowBanner;

/// The context threaded through the screen stack: the durable run state, the one
/// shared answer field (it owns a system font, so it lives here rather than per
/// screen), the pixel-art text style, the virtual screen size (for the banners to
/// recover the fit factor), and a quit flag the host loop watches.
pub struct Ctx {
    pub session: MathgameSession,
    pub input: InputField,
    pub text: TextStyle,
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
        virtual_size: Size,
        scores: HighScores,
        scores_cfg: ScoresConfig,
    ) -> Self {
        Self {
            session,
            input,
            text,
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

/// A centred big-text banner in the config text style.
fn banner(text: &str, style: TextStyle, virtual_size: Size) -> ShadowBanner {
    ShadowBanner::centered(text, style, virtual_size)
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
    ShadowBanner::at_virtual(
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

/// Play: the current equation as a banner, a score/lives HUD, and the shared
/// answer field. Enter grades the answer and either continues or ends the run.
struct PlayScreen {
    equation: ShadowBanner,
    hud: ShadowBanner,
    style: TextStyle,
    virtual_size: Size,
}

impl PlayScreen {
    fn new(session: &MathgameSession, style: TextStyle, virtual_size: Size) -> Self {
        Self {
            equation: banner(&session.current_prompt(), style, virtual_size),
            hud: hud(session, style, virtual_size),
            style,
            virtual_size,
        }
    }

    fn refresh(&mut self, session: &MathgameSession) {
        self.equation = banner(&session.current_prompt(), self.style, self.virtual_size);
        self.hud = hud(session, self.style, self.virtual_size);
    }
}

impl Screen<Ctx> for PlayScreen {
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
                let answer = ctx.input.submit();
                let report = ctx.session.submit_typed_answer(answer);
                match report.run_phase {
                    RunPhase::Playing => {
                        self.refresh(&ctx.session);
                        ScreenChange::None
                    }
                    phase => {
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
        overlays.push(&self.equation);
        overlays.push(&self.hud);
        overlays.push(&ctx.input);
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
            score: ShadowBanner::at_virtual(
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

        let mut lines = vec![ShadowBanner::at_virtual(
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
            lines.push(ShadowBanner::at_virtual(
                &text,
                Point::new(MARGIN_X, ROWS_TOP + i as i32 * ROW_PITCH),
                style.hud_scale,
                style,
                virtual_size,
            ));
        }

        let shown = scores.entries().len().min(capacity) as i32;
        lines.push(ShadowBanner::at_virtual(
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

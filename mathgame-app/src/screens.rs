//! The windowed shell: the shared screen context and the four screens
//! (title → name entry → play → result) driven on a `ScreenStack`.
//!
//! The durable run state lives in [`MathgameSession`]; a screen holds only local
//! UI state (a cached banner or HUD sprite). Input mutates the context
//! (`&mut Ctx`); rendering reads it (`&Ctx`). The pixel-art text style comes from
//! the config-loaded [`TextStyle`] threaded through the context, not constants.

use mathgame_app::MathgameSession;
use ratgames::{
    BigText, HighScores, InputField, OverlayLayer, PixelLayer, Placard, Point, RunPhase, Screen,
    ScreenChange, Sprite, Surface, UiInput,
};

use crate::config::{ScoresConfig, TextStyle};
use crate::scores;

/// The context threaded through the screen stack: the durable run state, the one
/// shared answer field (it owns a system font, so it lives here rather than per
/// screen), the pixel-art text style, and a quit flag the host loop watches.
pub struct Ctx {
    pub session: MathgameSession,
    pub input: InputField,
    pub text: TextStyle,
    pub scores: HighScores,
    pub scores_cfg: ScoresConfig,
    pub quit: bool,
}

impl Ctx {
    pub fn new(
        session: MathgameSession,
        input: InputField,
        text: TextStyle,
        scores: HighScores,
        scores_cfg: ScoresConfig,
    ) -> Self {
        Self {
            session,
            input,
            text,
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

/// A pixel-art text line drawn at a fixed top-left position — the HUD / score
/// line. Distinct from [`Placard`], which centres its sprite.
struct TextLine {
    sprite: Sprite,
    at: Point,
}

impl TextLine {
    fn new(text: &str, scale: u32, shadow_depth: u32, at: Point) -> Self {
        Self {
            sprite: BigText::new(scale).shadow_depth(shadow_depth).build(text),
            at,
        }
    }
}

impl PixelLayer for TextLine {
    fn render(&self, screen: &mut Surface) {
        screen.draw_sprite(&self.sprite, self.at);
    }
}

/// A centred big-text banner, styled by the config text style.
fn banner(text: &str, style: TextStyle) -> Placard {
    Placard::new(
        BigText::new(style.banner_scale)
            .shadow_depth(style.shadow_depth)
            .build(text),
    )
}

/// The top-of-screen score / lives / level line.
fn hud(session: &MathgameSession, style: TextStyle) -> TextLine {
    let run = session.run();
    let text = format!(
        "SCORE {}  LIVES {}  L{}",
        run.score().points(),
        run.lives().count(),
        run.levels().current() + 1,
    );
    TextLine::new(&text, style.hud_scale, style.shadow_depth, Point::new(4, 4))
}

/// Title screen: a banner. Enter starts, Esc quits.
pub struct TitleScreen {
    banner: Placard,
}

impl TitleScreen {
    #[must_use]
    pub fn new(style: TextStyle) -> Self {
        Self {
            banner: banner("MATH GAME", style),
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
        world: &mut Vec<&'a dyn PixelLayer>,
        _overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        world.push(&self.banner);
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
                ScreenChange::Replace(Box::new(PlayScreen::new(&ctx.session, ctx.text)))
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
    equation: Placard,
    hud: TextLine,
    style: TextStyle,
}

impl PlayScreen {
    fn new(session: &MathgameSession, style: TextStyle) -> Self {
        Self {
            equation: banner(&session.current_prompt(), style),
            hud: hud(session, style),
            style,
        }
    }

    fn refresh(&mut self, session: &MathgameSession) {
        self.equation = banner(&session.current_prompt(), self.style);
        self.hud = hud(session, self.style);
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
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        world.push(&self.equation);
        world.push(&self.hud);
        overlays.push(&ctx.input);
    }
}

/// Result: a win / game-over banner and the final score. Enter restarts.
struct ResultScreen {
    banner: Placard,
    score: TextLine,
}

impl ResultScreen {
    fn new(session: &MathgameSession, phase: RunPhase, style: TextStyle) -> Self {
        let title = if phase == RunPhase::Won {
            "YOU WIN"
        } else {
            "GAME OVER"
        };
        let score = format!("SCORE {}   ENTER", session.run().score().points());
        Self {
            banner: banner(title, style),
            score: TextLine::new(
                &score,
                style.hud_scale,
                style.shadow_depth,
                Point::new(4, 4),
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
        world: &mut Vec<&'a dyn PixelLayer>,
        _overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        world.push(&self.banner);
        world.push(&self.score);
    }
}

/// High scores: the ranked board shown after a run ends. Enter resets and returns
/// to the title; Esc quits.
struct HighScoreScreen {
    lines: Vec<TextLine>,
}

impl HighScoreScreen {
    /// A header, one row per board entry (up to `capacity`), and a footer hint —
    /// all pixel-art lines in the config text style, left-anchored.
    fn new(scores: &HighScores, style: TextStyle, capacity: usize) -> Self {
        const MARGIN_X: i32 = 8;
        const HEADER_Y: i32 = 4;
        const ROWS_TOP: i32 = 30;
        const ROW_PITCH: i32 = 13;
        const NAME_WIDTH: usize = 8;

        let mut lines = vec![TextLine::new(
            "HIGH SCORES",
            style.banner_scale,
            style.shadow_depth,
            Point::new(MARGIN_X, HEADER_Y),
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
            lines.push(TextLine::new(
                &text,
                style.hud_scale,
                style.shadow_depth,
                Point::new(MARGIN_X, ROWS_TOP + i as i32 * ROW_PITCH),
            ));
        }

        let shown = scores.entries().len().min(capacity) as i32;
        lines.push(TextLine::new(
            "PRESS ENTER",
            style.hud_scale,
            style.shadow_depth,
            Point::new(MARGIN_X, ROWS_TOP + shown * ROW_PITCH + 6),
        ));

        Self { lines }
    }
}

impl Screen<Ctx> for HighScoreScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Confirm => {
                ctx.session.reset();
                ScreenChange::Replace(Box::new(TitleScreen::new(ctx.text)))
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
        world: &mut Vec<&'a dyn PixelLayer>,
        _overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        for line in &self.lines {
            world.push(line);
        }
    }
}

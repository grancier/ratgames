//! The windowed shell: the shared screen context and the screens (title →
//! difficulty select → name entry → level intro → play → level clear → result →
//! high scores) driven on a `ScreenStack`. The gauntlet loops level-intro →
//! play → level-clear per level until the run is won or lost; a game over with
//! a continue to spend detours through the CONTINUE? prompt, and an idle title
//! slips into the attract loop (high scores ↔ how-to) until a key wakes it.
//!
//! The durable run state lives in [`MathgameSession`]; a screen holds only local
//! UI state (its cached banners, and any auto-advance countdown). Input mutates
//! the context (`&mut Ctx`); rendering reads it (`&Ctx`). The pixel-art text style
//! and the virtual screen size come from the config, threaded through the
//! context, not constants.
//!
//! Every text element is a [`ShadowBanner`] — an [`OverlayLayer`] that draws crisp
//! integer-scaled 8-bit glyphs with a real device-space drop shadow. The app
//! therefore pushes nothing to the pixel `world`; the banners composite over the
//! upscaled backdrop, anchored to the game viewport so they track the window and
//! letterbox exactly as the old pixel layers did.

use mathgame_app::{AttemptReport, MathLevel, MathgameSession};
use ratgames::{
    AttractCard, AttractLoop, BannerAnchor, Blink, BoardFooter, BoardLine, ChoiceList,
    ContinueRules, Countdown, CountdownConfig, FeedbackBeat, FeedbackBeatLayers, GlyphSource,
    HighScoreBoard, HighScoreBoardSpec, HighScoreLayout, HighScores, InputField,
    JsonHighScoreStore, LevelOutcome, MeterBar, OverlayLayer, PixelLayer, Point, RankRules, Rect,
    RunPhase, ScoringRules, Screen, ScreenChange, ShadowBanner, ShadowBannerFactory, Size,
    TimedCard, TimedCardExit, UiInput, accuracy_percent,
};

use crate::config::{AttractConfig, DifficultyPreset, FeedbackConfig, TextStyle, TimerBarConfig};
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
    /// The per-question timer bar's colours (the reusable gauge is [`MeterBar`];
    /// the bar's on-screen rect is an app layout constant).
    pub timer_bar: TimerBarConfig,
    /// The countdown config the Level Intro / Level Clear screens auto-advance on.
    pub interstitial: CountdownConfig,
    pub virtual_size: Size,
    /// The in-memory board, persisted through `store` as runs place.
    pub scores: HighScores,
    /// The persistence seam for `scores`, bound to the config path at startup.
    pub store: JsonHighScoreStore,
    /// The board's "top N" cap, applied when recording (a board never stores it).
    pub capacity: usize,
    /// Frames per second the host paces at — the unit for the question timer's
    /// budget and the per-second time bonus.
    pub frames_per_second: u32,
    /// Points per whole second left when a question is answered correctly.
    pub time_bonus_per_second: u32,
    /// Rank-based endings, proudest first; the result screen shows the first
    /// rank the finished run earns, or the plain win / game-over title.
    pub ranks: RankRules,
    /// How long the game-over CONTINUE? prompt holds before declining. (Whether a
    /// continue is offered at all is the session's policy: [`MathgameSession::can_continue`].)
    pub continue_prompt: CountdownConfig,
    /// Attract-mode timing: the title's idle trigger and the per-card hold.
    pub attract: AttractConfig,
    /// The selectable difficulties, in menu order; empty skips the select screen.
    pub difficulties: Vec<DifficultyPreset>,
    /// The gauntlet as authored — kept so a difficulty selection can rebuild the
    /// session with scaled time limits.
    pub levels: Vec<MathLevel>,
    /// The scoring policy, re-applied to a rebuilt session.
    pub scoring: ScoringRules,
    /// The continue policy, re-applied to a rebuilt session.
    pub continues: ContinueRules,
    /// The seed the next session rebuild draws its problem sequence from,
    /// bumped per rebuild so re-selecting a difficulty deals new problems.
    pub next_seed: u64,
    pub quit: bool,
}

impl Ctx {
    /// Record the finished run on the board and persist it — called once as a run
    /// ends, before the results and high-score screens read the board.
    fn record_run(&mut self) {
        let name = self.session.profile().name().to_string();
        let points = self.session.run().score().points();
        scores::record_and_save(&self.store, &mut self.scores, &name, points, self.capacity);
    }

    /// Rebuild the session for the chosen difficulty: the authored gauntlet with
    /// its time limits scaled and the preset's starting lives, under the same
    /// scoring and continue policies. The config was validated at startup
    /// (labels, lives, the scoring lives-cap cross-check), so a rebuild can only
    /// fail on a bug — then the current session is kept and the run starts
    /// unchanged, with a warning.
    fn apply_difficulty(&mut self, index: usize) {
        let Some(preset) = self.difficulties.get(index) else {
            return;
        };
        let levels = scaled_levels(&self.levels, preset.time_percent);
        let seed = self.next_seed;
        self.next_seed = self.next_seed.wrapping_add(1);
        match MathgameSession::from_levels(&levels, preset.starting_lives, seed)
            .and_then(|session| session.with_scoring(self.scoring.clone()))
        {
            Ok(session) => self.session = session.with_continues(self.continues),
            Err(error) => eprintln!("warning: difficulty {:?} rejected: {error}", preset.label),
        }
    }
}

/// The gauntlet with every level's time limit scaled by `time_percent`
/// (100 = as authored, more = easier). An untimed level (`0` frames) stays
/// untimed, and the result saturates rather than overflowing.
fn scaled_levels(levels: &[MathLevel], time_percent: u32) -> Vec<MathLevel> {
    levels
        .iter()
        .map(|level| {
            let mut level = level.clone();
            let scaled = u64::from(level.rules.time_limit_frames) * u64::from(time_percent) / 100;
            level.rules.time_limit_frames = u32::try_from(scaled).unwrap_or(u32::MAX);
            level
        })
        .collect()
}

/// Build a [`ShadowBannerFactory`] in the app's pixel-art style: `source`'s glyphs
/// (a 32px Menlo raster in the shipped config) with the config's em-relative drop
/// shadow, anchored to the virtual screen. The reusable banner composition lives
/// in `ratgames`; this only supplies the app's glyph source and shadow. Callers
/// pass the per-banner magnification (the app's `banner_scale` / `hud_scale`).
fn banner_factory(
    source: &dyn GlyphSource,
    style: TextStyle,
    virtual_size: Size,
) -> ShadowBannerFactory<'_> {
    ShadowBannerFactory::new(source, style.shadow.style(), virtual_size)
}

/// The top-of-screen score / lives / level line, anchored top-left.
fn hud(session: &MathgameSession, factory: &ShadowBannerFactory, scale: u32) -> ShadowBanner {
    let run = session.run();
    let text = format!(
        "SCORE {}  LIVES {}  L{}",
        run.score().points(),
        run.lives().count(),
        run.levels().current() + 1,
    );
    factory.at(&text, Point::new(4, 4), scale)
}

/// Title screen: a banner. Enter starts, Esc quits — and left idle long enough,
/// it hands off to the attract loop (high scores, then how-to, cycling until any
/// key wakes it back here).
pub struct TitleScreen {
    banner: ShadowBanner,
    /// The attract trigger: expires after the configured idle and any input
    /// pushes it back. `None` when attract mode is off.
    idle: Option<Countdown>,
}

impl TitleScreen {
    #[must_use]
    pub fn new(
        source: &dyn GlyphSource,
        style: TextStyle,
        virtual_size: Size,
        idle: Option<Countdown>,
    ) -> Self {
        let factory = banner_factory(source, style, virtual_size);
        Self {
            banner: factory.centered("MATH GAME", style.banner_scale),
            idle,
        }
    }
}

impl Screen<Ctx> for TitleScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        // Any key is a sign of life: push the attract trigger back.
        if let Some(idle) = self.idle.as_mut() {
            idle.reset();
        }
        match input {
            UiInput::Confirm => {
                // With difficulties configured, pick one first; otherwise play
                // the gauntlet exactly as authored.
                if ctx.difficulties.is_empty() {
                    ctx.input.set_prompt("NAME: ");
                    ScreenChange::Replace(Box::new(NameEntryScreen))
                } else {
                    ScreenChange::Replace(Box::new(DifficultySelectScreen::new(ctx)))
                }
            }
            UiInput::Cancel => {
                ctx.quit = true;
                ScreenChange::None
            }
            _ => ScreenChange::None,
        }
    }

    fn tick(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        let idled = self.idle.as_mut().is_some_and(|idle| {
            idle.advance();
            idle.is_expired()
        });
        if idled {
            ScreenChange::Replace(attract_loop(ctx))
        } else {
            ScreenChange::None
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

/// A fresh title screen, its attract trigger armed from config.
fn title_screen(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    Box::new(TitleScreen::new(
        &*ctx.glyphs,
        ctx.text,
        ctx.virtual_size,
        ctx.attract.idle_countdown(),
    ))
}

/// The attract loop: the idle title's showcase — the high-score board, then a
/// how-to card, each held for the configured card time, cycling until any key
/// wakes the title. The cycling and rendering are `ratgames::AttractLoop`; the app
/// supplies the two cards and where waking leads.
fn attract_loop(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    let scores = baked_board(
        &ctx.scores,
        &*ctx.glyphs,
        ctx.text,
        ctx.capacity,
        ctx.virtual_size,
    )
    .into_banners();

    let factory = banner_factory(&*ctx.glyphs, ctx.text, ctx.virtual_size);
    let line =
        |text: &str, y: i32| factory.at(text, Point::new(LEVEL_SCREEN_X, y), ctx.text.hud_scale);
    let howto = vec![
        factory.at(
            "HOW TO PLAY",
            Point::new(LEVEL_SCREEN_X, 40),
            ctx.text.banner_scale,
        ),
        line("SOLVE THE EQUATION", 150),
        line("TYPE THE ANSWER OR PICK ONE", 200),
        line("BEAT THE CLOCK FOR BONUS POINTS", 250),
        line("ENTER STARTS  ESC QUITS", 310),
    ];

    Box::new(AttractLoop::new(
        vec![
            AttractCard::new(scores, ctx.attract.card.countdown()),
            AttractCard::new(howto, ctx.attract.card.countdown()),
        ],
        |ctx: &mut Ctx| ScreenChange::Replace(title_screen(ctx)),
    ))
}

/// Vertical spacing of the difficulty menu rows, matching the choice list's.
const DIFFICULTY_ROW_PITCH: i32 = 46;

/// Difficulty select: a caret menu over the config's presets. Arrows move,
/// Enter rebuilds the run for the chosen preset and moves on to name entry,
/// Esc quits. Shown only when at least one preset is configured.
struct DifficultySelectScreen {
    banner: ShadowBanner,
    choices: ChoiceList,
}

impl DifficultySelectScreen {
    fn new(ctx: &Ctx) -> Self {
        let factory = banner_factory(&*ctx.glyphs, ctx.text, ctx.virtual_size);
        let labels: Vec<String> = ctx
            .difficulties
            .iter()
            .map(|preset| preset.label.clone())
            .collect();
        Self {
            banner: factory.at(
                "SELECT DIFFICULTY",
                Point::new(LEVEL_SCREEN_X, 40),
                ctx.text.banner_scale,
            ),
            choices: ChoiceList::new(
                labels,
                Point::new(LEVEL_SCREEN_X, 150),
                DIFFICULTY_ROW_PITCH,
                ctx.text.hud_scale,
                &factory,
            ),
        }
    }
}

impl Screen<Ctx> for DifficultySelectScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Cancel => {
                ctx.quit = true;
                ScreenChange::None
            }
            // Arrows navigate; Confirm returns the chosen index.
            other => {
                let chosen = {
                    let factory = banner_factory(&*ctx.glyphs, ctx.text, ctx.virtual_size);
                    self.choices.handle(other, &factory)
                };
                match chosen {
                    Some(index) => {
                        ctx.apply_difficulty(index);
                        ctx.input.set_prompt("NAME: ");
                        ScreenChange::Replace(Box::new(NameEntryScreen))
                    }
                    None => ScreenChange::None,
                }
            }
        }
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        _world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        overlays.push(&self.banner);
        overlays.push(&self.choices);
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
                ScreenChange::Replace(level_intro_screen(
                    &ctx.session,
                    &*ctx.glyphs,
                    ctx.text,
                    ctx.virtual_size,
                    ctx.interstitial.countdown(),
                ))
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

/// What to do when the feedback beat ends: reveal the next problem, celebrate a
/// cleared level, or leave for the result screen because the run finished.
#[derive(Debug, PartialEq, Eq)]
enum Pending {
    /// Stay on this level: reveal the next problem (or retry after a lost life).
    Advance,
    /// This answer cleared the level and the run plays on: show the Level Clear
    /// tally, then the next level's intro.
    LevelCleared,
    /// The run finished on this answer (won or game over): show the result.
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

/// What the beat does when it ends: a cleared level (run continuing) shows the
/// Level Clear tally; a finished run hands off to the result screen; anything
/// else (a next problem, or a retry after a lost life) stays on this level.
fn pending_for(report: &AttemptReport) -> Pending {
    match report.run_phase {
        RunPhase::Playing if report.level_outcome == LevelOutcome::Cleared => Pending::LevelCleared,
        RunPhase::Playing => Pending::Advance,
        finished => Pending::Finish(finished),
    }
}

/// Bake the flashing red reject cross: the same "X" glyph as the banner letters
/// (from `source`), as a tight red sprite scaled by `cross_scale` and blinked per
/// `cross_blink`. `GlyphMask::to_sprite` crops to the glyph's ink so the lone "X"
/// centres cleanly (a `BigText` bake would pad and blob it).
fn reject_cross(cfg: &FeedbackConfig, source: &dyn GlyphSource, virtual_size: Size) -> Blink {
    let cross = source.glyph('X').to_sprite(cfg.wrong_color);
    let blink = Blink::new(cross, BannerAnchor::Center, virtual_size).scale(cfg.cross_scale);
    cfg.cross_blink.apply(blink)
}

/// The active answer-feedback beat and what to do when it ends. The beat itself —
/// the reject blink, the verdict banner, the fading success wash, and their timing
/// — is the reusable [`FeedbackBeat`]; `pending` is the app's next step, applied by
/// [`resolve_feedback`](PlayScreen::resolve_feedback) once the beat is done. The
/// answer field is frozen throughout.
struct Feedback {
    beat: FeedbackBeat,
    pending: Pending,
}

/// The multiple-choice list for the session's current problem — a left-anchored
/// pixel-art [`ChoiceList`], or `None` in typed mode (which uses the shared answer
/// field instead). The layout values stay app-side, like the high-score board's.
fn choices_for(
    session: &MathgameSession,
    factory: &ShadowBannerFactory,
    scale: u32,
) -> Option<ChoiceList> {
    const CHOICES_X: i32 = 40;
    const CHOICES_Y: i32 = 150;
    const ROW_PITCH: i32 = 46;
    let labels = session.current_choices()?;
    Some(ChoiceList::new(
        labels,
        Point::new(CHOICES_X, CHOICES_Y),
        ROW_PITCH,
        scale,
        factory,
    ))
}

/// The equation banner, placed for the answer mode: centred (above the bottom
/// input field) in typed mode, or anchored near the top — clear of the choice
/// list below it — in multiple-choice mode.
fn equation_banner(
    session: &MathgameSession,
    factory: &ShadowBannerFactory,
    scale: u32,
) -> ShadowBanner {
    let prompt = session.current_prompt();
    if session.current_choices().is_some() {
        factory.at(&prompt, Point::new(40, 40), scale)
    } else {
        factory.centered(&prompt, scale)
    }
}

/// The per-question countdown for the level in play, or `None` when the level is
/// untimed (`time_limit_frames == 0`). Armed fresh for each problem.
fn question_timer(session: &MathgameSession) -> Option<Countdown> {
    let frames = session.current_time_limit_frames();
    (frames > 0).then(|| Countdown::new(frames))
}

/// The on-screen rectangle of the per-question time bar: a thin strip across the
/// lower part of the 640×360 virtual screen, its left edge aligned with the choice
/// list. A first-cut layout the visual pass can reposition; the colours come from
/// config.
const TIMER_BAR_RECT: Rect = Rect::new(Point::new(40, 330), Size::new(560, 12));

/// The per-question time bar for the level in play, or `None` on an untimed level —
/// paired with [`question_timer`]. Built full; [`PlayScreen::tick`] drains it to the
/// countdown's remaining fraction each frame.
fn question_timer_bar(session: &MathgameSession, colors: TimerBarConfig) -> Option<MeterBar> {
    let frames = session.current_time_limit_frames();
    (frames > 0).then(|| MeterBar::new(TIMER_BAR_RECT, colors.fill_color, colors.track_color))
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
    choices: Option<ChoiceList>,
    /// The per-question countdown, or `None` on an untimed level. Ticks only while
    /// the player is answering (frozen during the feedback beat); on expiry the
    /// question is a timed-out miss.
    timer: Option<Countdown>,
    /// The draining time bar mirroring `timer` (or `None` on an untimed level),
    /// pushed into the pixel `world` while answering. [`tick`](Self::tick) sets its
    /// fraction to the countdown's remaining / total each frame.
    timer_bar: Option<MeterBar>,
    /// The timer bar's colours, kept so [`refresh`](Self::refresh) can rebuild the
    /// bar for each new problem.
    timer_bar_colors: TimerBarConfig,
    /// This screen plays one level; its name (for the Level Clear tally) and the
    /// hit / miss tally over the whole level (for its accuracy) are captured here.
    level_name: String,
    hits: u32,
    misses: u32,
}

impl PlayScreen {
    fn new(
        session: &MathgameSession,
        source: &dyn GlyphSource,
        style: TextStyle,
        virtual_size: Size,
        timer_bar_colors: TimerBarConfig,
    ) -> Self {
        let factory = banner_factory(source, style, virtual_size);
        Self {
            equation: equation_banner(session, &factory, style.banner_scale),
            hud: hud(session, &factory, style.hud_scale),
            style,
            virtual_size,
            feedback: None,
            choices: choices_for(session, &factory, style.hud_scale),
            timer: question_timer(session),
            timer_bar: question_timer_bar(session, timer_bar_colors),
            timer_bar_colors,
            level_name: session.current_level_name().to_string(),
            hits: 0,
            misses: 0,
        }
    }

    fn refresh(&mut self, source: &dyn GlyphSource, session: &MathgameSession) {
        let factory = banner_factory(source, self.style, self.virtual_size);
        self.equation = equation_banner(session, &factory, self.style.banner_scale);
        self.hud = hud(session, &factory, self.style.hud_scale);
        self.choices = choices_for(session, &factory, self.style.hud_scale);
        self.timer = question_timer(session);
        self.timer_bar = question_timer_bar(session, self.timer_bar_colors);
    }

    /// Open the feedback beat for a graded answer or a timeout: refresh the HUD so
    /// the new score / lives show behind it, arm a miss's flashing cross or a hit's
    /// success wash, bake the `verdict` banner, and record what to do when the beat
    /// ends.
    fn begin_feedback(&mut self, ctx: &Ctx, report: &AttemptReport, verdict: &str) {
        let cfg = ctx.feedback;
        let source = &*ctx.glyphs;
        let factory = banner_factory(source, self.style, self.virtual_size);
        self.hud = hud(&ctx.session, &factory, self.style.hud_scale);
        // A miss opens with the flashing reject cross; a hit tints the screen with a
        // fading success wash. Both then hold the verdict for `duration_frames`.
        let (reject, wash) = if report.correct {
            (None, Some(cfg.correct_color))
        } else {
            (Some(reject_cross(&cfg, source, self.virtual_size)), None)
        };
        self.feedback = Some(Feedback {
            beat: FeedbackBeat::new(
                reject,
                wash,
                factory.centered(verdict, self.style.banner_scale),
                Countdown::new(cfg.duration_frames),
            ),
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
            Pending::LevelCleared => ScreenChange::Replace(level_clear_screen(
                &self.level_name,
                ctx.session.run().score().points(),
                self.hits,
                self.misses,
                &*ctx.glyphs,
                ctx.text,
                ctx.virtual_size,
                ctx.interstitial.countdown(),
            )),
            Pending::Finish(phase) => {
                // A game over with a continue to spend detours through the
                // CONTINUE? prompt — the run is not recorded yet, because a
                // continued run plays on. Every other ending records here.
                if phase == RunPhase::GameOver && ctx.session.can_continue() {
                    ScreenChange::Replace(continue_screen(ctx))
                } else {
                    ctx.record_run();
                    ScreenChange::Replace(result_screen(ctx, phase))
                }
            }
        }
    }

    /// The current question ran out of time: record it as a miss (no answer) and
    /// open the feedback beat with a "TIME UP" verdict — the same beat a wrong
    /// answer gets, so the run sequences identically.
    fn begin_timeout(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        let report = ctx.session.time_out();
        self.misses += 1;
        self.begin_feedback(ctx, &report, "TIME UP");
        ScreenChange::None
    }

    /// The time bonus for a correct answer given the clock left: whole seconds
    /// remaining times the configured per-second award. Zero on an untimed level.
    fn time_bonus(&self, ctx: &Ctx) -> u32 {
        match &self.timer {
            Some(timer) if ctx.frames_per_second > 0 => {
                (timer.remaining() / ctx.frames_per_second) * ctx.time_bonus_per_second
            }
            _ => 0,
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
                    ctx.session.submit_choice(choices.selected())
                } else {
                    let answer = ctx.input.submit();
                    ctx.session.submit_typed_answer(answer)
                };
                // Tally this level's hits / misses for the Level Clear accuracy, and
                // reward a correct answer with a time bonus for the seconds to spare.
                if report.correct {
                    self.hits += 1;
                    let bonus = self.time_bonus(ctx);
                    ctx.session.award_bonus(bonus);
                } else {
                    self.misses += 1;
                }
                self.begin_feedback(ctx, &report, &verdict_line(&report));
                ScreenChange::None
            }
            UiInput::Cancel => {
                ctx.quit = true;
                ScreenChange::None
            }
            // Everything else navigates the choice list (arrows) or edits the typed
            // line (type/backspace/delete/caret movement).
            other => {
                let (style, virtual_size) = (self.style, self.virtual_size);
                if let Some(choices) = self.choices.as_mut() {
                    let factory = banner_factory(&*ctx.glyphs, style, virtual_size);
                    choices.handle(other, &factory);
                } else {
                    ctx.input.handle(other);
                }
                ScreenChange::None
            }
        }
    }

    fn tick(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        // A feedback beat, if running, takes priority and freezes the question timer.
        if self.feedback.is_some() {
            let done = self.feedback.as_mut().is_some_and(|f| f.beat.advance());
            return if done {
                self.resolve_feedback(ctx)
            } else {
                ScreenChange::None
            };
        }
        // Otherwise run the question timer (on a timed level), draining the visible
        // bar to what's left; running out of time is a timed-out miss.
        let mut timed_out = false;
        if let Some(timer) = self.timer.as_mut() {
            timer.advance();
            timed_out = timer.is_expired();
            let (remaining, total) = (timer.remaining(), timer.total());
            if let Some(bar) = self.timer_bar.as_mut() {
                bar.set_fraction(remaining, total);
            }
        }
        if timed_out {
            self.begin_timeout(ctx)
        } else {
            ScreenChange::None
        }
    }

    fn collect_layers<'a>(
        &'a self,
        ctx: &'a Ctx,
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        match &self.feedback {
            Some(feedback) => match feedback.beat.layers() {
                // Opening phase: the reject blink over the frozen problem.
                FeedbackBeatLayers::Opening { reject } => {
                    overlays.push(&self.equation);
                    overlays.push(&self.hud);
                    overlays.push(reject);
                }
                // Verdict phase: the verdict (and a hit's fading wash) over the HUD.
                FeedbackBeatLayers::Verdict { wash, verdict } => {
                    if let Some(wash) = wash {
                        overlays.push(wash);
                    }
                    overlays.push(&self.hud);
                    overlays.push(verdict);
                }
                // Finished — the tick that ends the beat resolves it, so this frame
                // is not normally reached; contribute nothing beat-specific.
                FeedbackBeatLayers::Done => {}
            },
            None => {
                // The draining time bar lives in the pixel world, beneath the
                // overlays; shown only while answering (omitted during the feedback
                // beat, like the frozen timer it mirrors).
                if let Some(bar) = &self.timer_bar {
                    world.push(bar);
                }
                overlays.push(&self.equation);
                overlays.push(&self.hud);
                match &self.choices {
                    Some(choices) => overlays.push(choices),
                    None => overlays.push(&ctx.input),
                }
            }
        }
    }
}

/// Left margin for the level-interstitial text, matching the choice list.
const LEVEL_SCREEN_X: i32 = 40;

/// Level Intro card: a brief "ROUND N OF M" interstitial with the level's theme
/// name, difficulty, and target, shown before each level on a [`TimedCard`]. It
/// holds until the countdown expires then auto-advances into play; Enter skips the
/// wait, Esc quits. The banners are app-styled; the hold + input mechanic is the
/// reusable card.
fn level_intro_screen(
    session: &MathgameSession,
    source: &dyn GlyphSource,
    style: TextStyle,
    virtual_size: Size,
    countdown: Countdown,
) -> Box<dyn Screen<Ctx>> {
    let levels = session.run().levels();
    let round = levels.current() + 1;
    // Left-anchored hud-scale lines, like the HUD and choice list — a first cut the
    // visual pass can re-scale/reposition.
    let factory = banner_factory(source, style, virtual_size);
    let line =
        |text: &str, y: i32| factory.at(text, Point::new(LEVEL_SCREEN_X, y), style.hud_scale);
    let banners = vec![
        line(&format!("ROUND {round} OF {}", levels.total()), 70),
        line(session.current_level_name(), 140),
        line(
            &format!(
                "{}  GET {} RIGHT",
                session.current_difficulty(),
                session.goal().required_successes()
            ),
            210,
        ),
    ];
    // Confirm or expiry begins play for the now-current level (built fresh from the
    // context at exit time); cancel quits.
    Box::new(TimedCard::new(
        banners,
        countdown,
        |exit, ctx: &mut Ctx| match exit {
            TimedCardExit::Cancelled => {
                ctx.quit = true;
                ScreenChange::None
            }
            TimedCardExit::Confirmed | TimedCardExit::Expired => {
                ScreenChange::Replace(Box::new(PlayScreen::new(
                    &ctx.session,
                    &*ctx.glyphs,
                    ctx.text,
                    ctx.virtual_size,
                    ctx.timer_bar,
                )))
            }
        },
    ))
}

/// Level Clear card: the just-cleared level's tally — its name, the running score,
/// and this level's accuracy — on a [`TimedCard`]. It holds until the countdown
/// expires then auto-advances into the next level's intro; Enter skips the wait,
/// Esc quits.
#[allow(clippy::too_many_arguments)]
fn level_clear_screen(
    level_name: &str,
    score: u32,
    hits: u32,
    misses: u32,
    source: &dyn GlyphSource,
    style: TextStyle,
    virtual_size: Size,
    countdown: Countdown,
) -> Box<dyn Screen<Ctx>> {
    let factory = banner_factory(source, style, virtual_size);
    let line =
        |text: &str, y: i32| factory.at(text, Point::new(LEVEL_SCREEN_X, y), style.hud_scale);
    let banners = vec![
        line("LEVEL CLEAR", 50),
        line(level_name, 120),
        line(&format!("SCORE {score}"), 190),
        line(
            &format!("ACCURACY {}%", accuracy_percent(hits, misses)),
            250,
        ),
    ];
    // Confirm or expiry moves on to the next level's intro (the run has already
    // advanced to it), built fresh from the context; cancel quits.
    Box::new(TimedCard::new(
        banners,
        countdown,
        |exit, ctx: &mut Ctx| match exit {
            TimedCardExit::Cancelled => {
                ctx.quit = true;
                ScreenChange::None
            }
            TimedCardExit::Confirmed | TimedCardExit::Expired => {
                ScreenChange::Replace(level_intro_screen(
                    &ctx.session,
                    &*ctx.glyphs,
                    ctx.text,
                    ctx.virtual_size,
                    ctx.interstitial.countdown(),
                ))
            }
        },
    ))
}

/// Where the game-over CONTINUE? prompt's countdown digit sits — roughly centred
/// under the centred banner. A first-cut layout the visual pass can reposition,
/// like [`TIMER_BAR_RECT`].
const CONTINUE_SECONDS_AT: Point = Point::new(300, 240);

/// The game-over CONTINUE? prompt: a [`TimedCard`] holding a centred banner and a
/// live seconds readout. Enter spends a continue and resumes the run on its
/// current level (via that level's intro); letting the countdown run out declines
/// and moves on to the result. Esc still quits — the finished run is recorded on
/// both leaving paths, and NOT when it continues (it plays on).
fn continue_screen(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    let factory = banner_factory(&*ctx.glyphs, ctx.text, ctx.virtual_size);
    let banners = vec![
        factory.centered("CONTINUE?", ctx.text.banner_scale),
        factory.at(
            &format!(
                "ENTER TO CONTINUE  {} LEFT",
                ctx.session.continues_remaining()
            ),
            Point::new(LEVEL_SCREEN_X, 320),
            ctx.text.hud_scale,
        ),
    ];
    let (style, virtual_size) = (ctx.text, ctx.virtual_size);
    Box::new(
        TimedCard::new(
            banners,
            ctx.continue_prompt.countdown(),
            |exit, ctx: &mut Ctx| {
                match exit {
                    TimedCardExit::Confirmed if ctx.session.continue_run() => {
                        ScreenChange::Replace(level_intro_screen(
                            &ctx.session,
                            &*ctx.glyphs,
                            ctx.text,
                            ctx.virtual_size,
                            ctx.interstitial.countdown(),
                        ))
                    }
                    // Declined (the hold ran out), or a continue that could not be
                    // spent: the run is over — record it and show the result.
                    TimedCardExit::Confirmed | TimedCardExit::Expired => {
                        ctx.record_run();
                        ScreenChange::Replace(result_screen(ctx, RunPhase::GameOver))
                    }
                    // Esc quits, as everywhere — but the finished run still records.
                    TimedCardExit::Cancelled => {
                        ctx.record_run();
                        ctx.quit = true;
                        ScreenChange::None
                    }
                }
            },
        )
        .with_seconds(ctx.frames_per_second, move |secs, ctx: &Ctx| {
            let factory = banner_factory(&*ctx.glyphs, style, virtual_size);
            factory.at(&secs.to_string(), CONTINUE_SECONDS_AT, style.banner_scale)
        }),
    )
}

/// The ending title for a finished run: the first rank the run earned, or the
/// plain phase title. Pure, so it is unit-tested directly.
fn ending_title(phase: RunPhase, rank: Option<&str>) -> &str {
    rank.unwrap_or(if phase == RunPhase::Won {
        "YOU WIN"
    } else {
        "GAME OVER"
    })
}

/// Result: the ending banner — the run's earned rank ("MATH MASTER"), or the
/// plain win / game-over title — and the final score. Enter shows the board.
struct ResultScreen {
    banner: ShadowBanner,
    score: ShadowBanner,
}

impl ResultScreen {
    fn new(
        session: &MathgameSession,
        source: &dyn GlyphSource,
        phase: RunPhase,
        rank: Option<&str>,
        style: TextStyle,
        virtual_size: Size,
    ) -> Self {
        let score = format!("SCORE {}   ENTER", session.run().score().points());
        let factory = banner_factory(source, style, virtual_size);
        Self {
            banner: factory.centered(ending_title(phase, rank), style.banner_scale),
            score: factory.at(&score, Point::new(4, 4), style.hud_scale),
        }
    }
}

/// The result screen for the run as it stands, ranked against the configured
/// endings — built from the context wherever a run finishes.
fn result_screen(ctx: &Ctx, phase: RunPhase) -> Box<dyn Screen<Ctx>> {
    Box::new(ResultScreen::new(
        &ctx.session,
        &*ctx.glyphs,
        phase,
        ctx.session.rank(&ctx.ranks),
        ctx.text,
        ctx.virtual_size,
    ))
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

/// Bake the ranked board in the app's layout: a "HIGH SCORES" header, the
/// entries (up to `capacity`) in two columns, and a "PRESS ENTER" footer — all
/// banners in the config text style, anchored to virtual-screen positions. Two
/// columns because at 32px a ten-row board is far taller than the 360px screen;
/// five per column fits comfortably. Shared by the post-run high-score screen
/// and the attract loop.
fn baked_board(
    scores: &HighScores,
    source: &dyn GlyphSource,
    style: TextStyle,
    capacity: usize,
    virtual_size: Size,
) -> HighScoreBoard {
    const MARGIN_X: i32 = 16;
    const HEADER_Y: i32 = 8;
    const FOOTER_GAP: i32 = 12;

    // ratgames grid-places and bakes the ranked rows; the app supplies the
    // layout values, its banner style, and the header / footer copy.
    let layout = HighScoreLayout {
        origin: Point::new(MARGIN_X, 60),
        row_pitch: 36,
        column_width: 300,
        rows_per_column: 5,
        name_width: 5,
    };
    let factory = banner_factory(source, style, virtual_size);
    HighScoreBoard::new(
        scores,
        &factory,
        HighScoreBoardSpec {
            layout,
            capacity,
            row_scale: style.hud_scale,
            header: Some(BoardLine {
                text: "HIGH SCORES",
                at: Point::new(MARGIN_X, HEADER_Y),
                scale: style.banner_scale,
            }),
            footer: Some(BoardFooter {
                text: "PRESS ENTER",
                gap_below_rows: FOOTER_GAP,
                scale: style.hud_scale,
            }),
        },
    )
}

/// High scores: the ranked board shown after a run ends. Enter resets and returns
/// to the title; Esc quits.
struct HighScoreScreen {
    board: HighScoreBoard,
}

impl HighScoreScreen {
    fn new(
        scores: &HighScores,
        source: &dyn GlyphSource,
        style: TextStyle,
        capacity: usize,
        virtual_size: Size,
    ) -> Self {
        Self {
            board: baked_board(scores, source, style, capacity, virtual_size),
        }
    }
}

impl Screen<Ctx> for HighScoreScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Confirm => {
                ctx.session.reset();
                ScreenChange::Replace(title_screen(ctx))
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
        self.board.collect_layers(overlays);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mathgame_core::{DirectArithmetic, Generator, Operator, Response, Rng, evaluate};
    use ratgames::{Bitmap8x8, BlinkConfig, Color, LevelOutcome};

    fn cfg() -> FeedbackConfig {
        FeedbackConfig {
            correct_color: Color::argb(0x99, 0x39, 0xD3, 0x53),
            wrong_color: Color::rgb(0xE0, 0x2C, 0x2C),
            duration_frames: 30,
            cross_scale: 8,
            cross_blink: BlinkConfig {
                blinks: 3,
                on_frames: 12,
                off_frames: 12,
            },
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
    fn scaled_levels_scale_only_the_authored_time_limits() {
        use mathgame_app::{Arithmetic, OperatorConfig};
        use ratgames::LevelSpec;

        let level = |frames: u32| MathLevel {
            name: "L".to_string(),
            difficulty: "EASY".to_string(),
            rules: LevelSpec {
                time_limit_frames: frames,
                ..LevelSpec::default()
            },
            content: Arithmetic {
                operator: OperatorConfig::Add,
                min: 0,
                max: 9,
            },
        };
        let levels = vec![level(600), level(0), level(u32::MAX)];

        let easier = scaled_levels(&levels, 150);
        assert_eq!(easier[0].rules.time_limit_frames, 900);
        assert_eq!(
            easier[1].rules.time_limit_frames, 0,
            "untimed stays untimed"
        );
        assert_eq!(easier[2].rules.time_limit_frames, u32::MAX, "saturates");

        let harder = scaled_levels(&levels, 75);
        assert_eq!(harder[0].rules.time_limit_frames, 450);

        let as_authored = scaled_levels(&levels, 100);
        assert_eq!(as_authored[0].rules.time_limit_frames, 600);
        // Everything but the time limit is untouched.
        assert_eq!(as_authored[0].name, "L");
        assert_eq!(
            as_authored[0].rules.required_successes,
            levels[0].rules.required_successes
        );
    }

    #[test]
    fn the_ending_title_prefers_the_earned_rank() {
        assert_eq!(
            ending_title(RunPhase::Won, Some("NO MISS CHAMP")),
            "NO MISS CHAMP"
        );
        assert_eq!(ending_title(RunPhase::Won, None), "YOU WIN");
        assert_eq!(ending_title(RunPhase::GameOver, None), "GAME OVER");
        // A rank on a lost run (a game may configure one) still shows.
        assert_eq!(
            ending_title(RunPhase::GameOver, Some("GOOD EFFORT")),
            "GOOD EFFORT"
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
        // blinks × (on + off) frames per cycle.
        let total =
            cfg.cross_blink.blinks * (cfg.cross_blink.on_frames + cfg.cross_blink.off_frames);
        let mut cross = reject_cross(&cfg, &Bitmap8x8, Size::new(256, 256));
        for _ in 0..total - 1 {
            cross.advance();
            assert!(!cross.is_done());
        }
        cross.advance();
        assert!(cross.is_done());
    }

    #[test]
    fn pending_routes_a_cleared_level_and_a_finished_run() {
        // A clear while the run plays on shows the Level Clear tally.
        let clear_playing = AttemptReport {
            correct: true,
            level_outcome: LevelOutcome::Cleared,
            run_phase: RunPhase::Playing,
            evaluation: None,
        };
        assert_eq!(pending_for(&clear_playing), Pending::LevelCleared);

        // A clear that also won the run goes to the result, not the tally.
        let clear_won = AttemptReport {
            correct: true,
            level_outcome: LevelOutcome::Cleared,
            run_phase: RunPhase::Won,
            evaluation: None,
        };
        assert_eq!(pending_for(&clear_won), Pending::Finish(RunPhase::Won));

        // An in-progress answer (or a retry after a lost life) stays on the level.
        assert_eq!(
            pending_for(&report(true, RunPhase::Playing, None)),
            Pending::Advance
        );
    }
}

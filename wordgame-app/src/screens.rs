//! The windowed shell: the shared screen context and the screens (title →
//! difficulty select → name entry → level intro → play → level clear → result →
//! high scores) driven on a `ScreenStack`. The gauntlet loops level-intro →
//! play → level-clear per level until the run is won or lost; a game over with
//! a continue to spend detours through the CONTINUE? prompt, and an idle title
//! slips into the attract loop (high scores ↔ how-to) until a key wakes it.
//!
//! The durable run state lives in [`WordgameSession`]; a screen holds only
//! local UI state (its cached banners, and any auto-advance countdown). Input
//! mutates the context (`&mut Ctx`); rendering reads it (`&Ctx`). The
//! pixel-art text style and the virtual screen size come from the config,
//! threaded through the context, not constants.
//!
//! Every text element is a [`ShadowBanner`] — an [`OverlayLayer`] that draws
//! crisp integer-scaled 8-bit glyphs with a real device-space drop shadow. The
//! app therefore pushes nothing to the pixel `world`; the banners composite
//! over the upscaled backdrop, anchored to the game viewport so they track the
//! window and letterbox exactly as pixel layers would.
//!
//! Answers are typed only — the masked word (`C_T`) centres over the shared
//! input field and the player supplies the missing letters one at a time —
//! so, unlike the mathgame twin of this module, there is no multiple-choice
//! list on the play screen.

use ratgames::{
    AttractCard, AttractConfig, AttractLoop, BannerAnchor, BannerColumn, BannerContext,
    BannerStyle, Blink, BoardFooter, Challenge, ChallengeAnswer, ChallengeResolution,
    ChallengeScreen, ChallengeView, ChoiceList, ChoiceScreen, ContinueExit, ContinuePrompt,
    ContinueRules, Countdown, CountdownConfig, FeedbackBeat, FeedbackBeatConfig, GlyphSource,
    GradedAttempt, HighScoreBoard, HighScoreBoardSpec, HighScores, InputContext, InputField,
    InputLine, JsonHighScoreStore, LevelOutcome, MeterBarConfig, OverlayLayer, Point, PromptExit,
    PromptScreen, RankRules, RunPhase, ScoringRules, Screen, ScreenChange, ShadowBanner,
    ShadowBannerFactory, Size, TextEntryExit, TextEntryScreen, TimedCard, TimedCardExit,
    TimedGauge, accuracy_percent, fill_placeholders,
};
use wordgame_app::{AttemptReport, WordLevel, WordgameSession};
use wordgame_core::WordList;

use crate::config::{CopyConfig, DifficultyPreset, LayoutConfig, ResultCopy, VerdictCopy};
use crate::scores;

/// The context threaded through the screen stack: the durable run state, the
/// one shared answer field (it owns a system font, so it lives here rather
/// than per screen), the pixel-art text style, the virtual screen size (for
/// the banners to recover the fit factor), and a quit flag the host loop
/// watches.
pub struct Ctx {
    pub session: WordgameSession,
    pub input: InputField,
    pub text: BannerStyle,
    /// The glyph source the display-height banners and the reject cross render
    /// through (a 64px Menlo raster in the shipped config), resolved once and
    /// shared.
    pub glyphs: Box<dyn GlyphSource>,
    /// The optional smaller glyph source for body-height text (the HUD line,
    /// lists, board rows, readouts — the `hud_scale` family); `None` shares
    /// `glyphs`, the single-source look.
    pub hud_glyphs: Option<Box<dyn GlyphSource>>,
    pub feedback: FeedbackBeatConfig,
    /// The per-question timer bar's colours — a reusable `ratgames` meter-bar
    /// config; the bar's on-screen rect comes from the layout config.
    pub timer_bar: MeterBarConfig,
    /// The countdown config the Level Intro / Level Clear screens auto-advance
    /// on.
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
    /// How long the game-over CONTINUE? prompt holds before declining.
    /// (Whether a continue is offered at all is the session's policy:
    /// [`WordgameSession::can_continue`].)
    pub continue_prompt: CountdownConfig,
    /// Attract-mode timing: the title's idle trigger and the per-card hold.
    pub attract: AttractConfig,
    /// The selectable difficulties, in menu order; empty skips the select
    /// screen.
    pub difficulties: Vec<DifficultyPreset>,
    /// Every user-facing string, from `copy.json` — no on-screen text is a
    /// Rust literal.
    pub copy: CopyConfig,
    /// Where every screen element sits, from `layout.json` — no position is a
    /// Rust literal.
    pub layout: LayoutConfig,
    /// The gauntlet as authored — kept so a difficulty selection can rebuild
    /// the session with scaled time limits.
    pub levels: Vec<WordLevel>,
    /// The word pool the gauntlet poses from — kept for the same rebuild.
    pub words: WordList,
    /// The scoring policy, re-applied to a rebuilt session.
    pub scoring: ScoringRules,
    /// The continue policy, re-applied to a rebuilt session.
    pub continues: ContinueRules,
    /// The seed the next session rebuild draws its puzzle sequence from,
    /// bumped per rebuild so re-selecting a difficulty deals new puzzles.
    pub next_seed: u64,
    pub quit: bool,
}

impl Ctx {
    /// The glyph source for body-height text — the hud source when configured,
    /// else the shared banner source.
    fn hud_source(&self) -> &dyn GlyphSource {
        self.hud_glyphs.as_deref().unwrap_or(&*self.glyphs)
    }

    /// Record the finished run on the board and persist it — called once as a
    /// run ends, before the results and high-score screens read the board.
    fn record_run(&mut self) {
        let name = self.session.profile().name().to_string();
        let points = self.session.run().score().points();
        scores::record_and_save(&self.store, &mut self.scores, &name, points, self.capacity);
    }

    /// Rebuild the session for the chosen difficulty: the authored gauntlet
    /// with its time limits scaled and the preset's starting lives, under the
    /// same scoring and continue policies. The config was validated at startup
    /// (labels, lives, the scoring lives-cap cross-check), so a rebuild can
    /// only fail on a bug — then the current session is kept and the run
    /// starts unchanged, with a warning.
    fn apply_difficulty(&mut self, index: usize) {
        let Some(preset) = self.difficulties.get(index) else {
            return;
        };
        let levels = scaled_levels(&self.levels, preset.time_percent);
        let seed = self.next_seed;
        self.next_seed = self.next_seed.wrapping_add(1);
        match WordgameSession::from_levels(&levels, &self.words, preset.starting_lives, seed)
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
fn scaled_levels(levels: &[WordLevel], time_percent: u32) -> Vec<WordLevel> {
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

/// Build a [`ShadowBannerFactory`] in the app's pixel-art style: `source`'s
/// glyphs with the config's em-relative drop shadow, anchored to the virtual
/// screen. The reusable banner composition lives in `ratgames`; this only
/// supplies the app's glyph source and shadow. Callers pass the per-banner
/// magnification (the app's `banner_scale` / `hud_scale`).
fn banner_factory(
    source: &dyn GlyphSource,
    style: BannerStyle,
    virtual_size: Size,
) -> ShadowBannerFactory<'_> {
    ShadowBannerFactory::new(source, style.shadow.style(), virtual_size)
}

/// The app's screen context hands `ratgames` screens its banner factory, so a
/// generic pixel-art screen that re-bakes on interaction (e.g. [`ChoiceScreen`])
/// composites in the app's own style. Delegates to the free `banner_factory`.
impl BannerContext for Ctx {
    fn banner_factory(&self) -> ShadowBannerFactory<'_> {
        banner_factory(&*self.glyphs, self.text, self.virtual_size)
    }

    fn hud_factory(&self) -> ShadowBannerFactory<'_> {
        banner_factory(self.hud_source(), self.text, self.virtual_size)
    }
}

/// The context likewise hands `ratgames` screens its one durable input field
/// through the text-entry seam: the editable line for editing / submit, the
/// drawn field for rendering.
impl InputContext for Ctx {
    fn input_line(&mut self) -> &mut InputLine {
        self.input.line_mut()
    }

    fn input_overlay(&self) -> &dyn OverlayLayer {
        &self.input
    }
}

/// The top-of-screen score / lives / level line, anchored top-left. `template`
/// is the copy's HUD format — three `{}` (score, lives, level).
fn hud(
    session: &WordgameSession,
    factory: &ShadowBannerFactory,
    scale: u32,
    template: &str,
    at: Point,
) -> ShadowBanner {
    let run = session.run();
    let text = fill_placeholders(
        template,
        &[
            run.score().points().to_string(),
            run.lives().count().to_string(),
            (run.levels().current() + 1).to_string(),
        ],
    );
    factory.at(&text, at, scale)
}

/// Title screen: a banner. Enter starts, Esc quits — and left idle long
/// enough, it hands off to the attract loop (high scores, then how-to, cycling
/// until any key wakes it back here). The static-card mechanism (banners +
/// one-shot confirm/cancel routing + the resettable idle trigger) is
/// `ratgames::PromptScreen`; the app supplies the title banner, the attract
/// timing, and where each exit leads.
pub fn title_screen(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    let banner = ctx
        .banner_factory()
        .centered(&ctx.copy.title, ctx.text.banner_scale);
    let screen = PromptScreen::new(vec![banner], |exit, ctx: &mut Ctx| match exit {
        PromptExit::Confirmed => {
            // With difficulties configured, pick one first; otherwise play the
            // gauntlet exactly as authored.
            if ctx.difficulties.is_empty() {
                ctx.input.set_prompt(&ctx.copy.name_prompt);
                ScreenChange::Replace(name_entry_screen())
            } else {
                ScreenChange::Replace(difficulty_select_screen(ctx))
            }
        }
        PromptExit::Cancelled => {
            ctx.quit = true;
            ScreenChange::None
        }
        PromptExit::Idled => ScreenChange::Replace(attract_loop(ctx)),
    });
    Box::new(match ctx.attract.idle_countdown() {
        Some(idle) => screen.with_idle(idle),
        None => screen,
    })
}

/// The attract loop: the idle title's showcase — the high-score board, then a
/// how-to card, each held for the configured card time, cycling until any key
/// wakes the title. The cycling and rendering are `ratgames::AttractLoop`; the
/// app supplies the two cards and where waking leads.
fn attract_loop(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    let scores = baked_board(ctx);

    // The card title is display text (banner source); the instruction lines
    // are body text (hud source).
    let mut howto = vec![ctx.banner_factory().at(
        &ctx.copy.howto.title,
        Point::new(ctx.layout.screen_x, ctx.layout.title_y),
        ctx.text.banner_scale,
    )];
    howto.extend(
        BannerColumn::at_x(ctx.layout.screen_x)
            .lines(
                &ctx.copy.howto.lines,
                &ctx.layout.howto_line_ys,
                ctx.text.hud_scale,
            )
            .bake(&ctx.hud_factory()),
    );

    Box::new(AttractLoop::new(
        vec![
            AttractCard::new(scores, ctx.attract.card.countdown()),
            AttractCard::new(howto, ctx.attract.card.countdown()),
        ],
        |ctx: &mut Ctx| ScreenChange::Replace(title_screen(ctx)),
    ))
}

/// Difficulty select: a caret menu over the config's presets. Arrows move,
/// Enter rebuilds the run for the chosen preset and moves on to name entry,
/// Esc quits. Shown only when at least one preset is configured. The menu
/// mechanism (title + caret list + navigation + routing) is
/// `ratgames::ChoiceScreen`; the app supplies the preset labels and what a
/// choice does — scale and rebuild the run, then name entry.
fn difficulty_select_screen(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    // The title is display text (banner source); the menu rows are body text
    // (hud source — the same factory their caret re-bakes through).
    let title = ctx.banner_factory().at(
        &ctx.copy.select_difficulty,
        Point::new(ctx.layout.screen_x, ctx.layout.title_y),
        ctx.text.banner_scale,
    );
    let labels: Vec<String> = ctx
        .difficulties
        .iter()
        .map(|preset| preset.label.clone())
        .collect();
    let choices = ChoiceList::new(
        labels,
        Point::new(ctx.layout.screen_x, ctx.layout.menu_y),
        ctx.layout.menu_row_pitch,
        ctx.text.hud_scale,
        &ctx.hud_factory(),
    );
    Box::new(ChoiceScreen::new(
        title,
        choices,
        |index, ctx: &mut Ctx| {
            ctx.apply_difficulty(index);
            ctx.input.set_prompt(&ctx.copy.name_prompt);
            ScreenChange::Replace(name_entry_screen())
        },
        |ctx: &mut Ctx| {
            ctx.quit = true;
            ScreenChange::None
        },
    ))
}

/// Name entry: type into the shared answer field; Enter records the name and
/// starts play. The text-entry mechanism (route editing to the field, commit
/// the entered line, one-shot routing) is `ratgames::TextEntryScreen` over the
/// `InputContext` seam; the app supplies the blank-name fallback, the prompt
/// swap, and the route into the run. Callers set the name prompt before entry.
fn name_entry_screen() -> Box<dyn Screen<Ctx>> {
    Box::new(TextEntryScreen::new(|exit, ctx: &mut Ctx| match exit {
        TextEntryExit::Submitted(name) => {
            let name = if name.trim().is_empty() {
                ctx.copy.default_player.clone()
            } else {
                name
            };
            ctx.session.set_player_name(name);
            ctx.input.set_prompt(&ctx.copy.answer_prompt);
            ScreenChange::Replace(level_intro_screen(ctx, ctx.interstitial.countdown()))
        }
        TextEntryExit::Cancelled => {
            ctx.quit = true;
            ScreenChange::None
        }
    }))
}

/// What to do when the feedback beat ends: reveal the next puzzle, celebrate a
/// cleared level, or leave for the result screen because the run finished.
#[derive(Debug, PartialEq, Eq)]
enum Pending {
    /// Stay on this level: reveal the next puzzle (or retry after a lost life).
    Advance,
    /// This answer cleared the level and the run plays on: show the Level
    /// Clear tally, then the next level's intro.
    LevelCleared,
    /// The run finished on this answer (won or game over): show the result.
    Finish(RunPhase),
}

/// The verdict line for a graded answer — the clarity-critical text. A hit
/// reads `CORRECT`; a miss reveals the whole word plainly (`THE WORD WAS CAT`,
/// never `WRONG CAT`, which reads as if CAT were the wrong answer). Pure and
/// font-free, so it is unit-tested directly.
fn verdict_line(report: &AttemptReport, verdict: &VerdictCopy) -> String {
    if report.correct {
        verdict.correct.clone()
    } else {
        match report.revealed.as_ref() {
            Some(word) => fill_placeholders(&verdict.answer_is, std::slice::from_ref(word)),
            None => verdict.wrong.clone(),
        }
    }
}

/// What the beat does when it ends: a cleared level (run continuing) shows the
/// Level Clear tally; a finished run hands off to the result screen; anything
/// else (a next puzzle, or a retry after a lost life) stays on this level.
fn pending_for(report: &AttemptReport) -> Pending {
    match report.run_phase {
        RunPhase::Playing if report.level_outcome == LevelOutcome::Cleared => Pending::LevelCleared,
        RunPhase::Playing => Pending::Advance,
        finished => Pending::Finish(finished),
    }
}

/// Bake the flashing red reject cross: the same "X" glyph as the banner
/// letters (from `source`), as a tight red sprite scaled by `cross_scale` and
/// blinked per `cross_blink`. `GlyphMask::to_sprite` crops to the glyph's ink
/// so the lone "X" centres cleanly (a `BigText` bake would pad and blob it).
fn reject_cross(cfg: &FeedbackBeatConfig, source: &dyn GlyphSource, virtual_size: Size) -> Blink {
    let cross = source.glyph('X').to_sprite(cfg.wrong_color);
    let blink = Blink::new(cross, BannerAnchor::Center, virtual_size).scale(cfg.cross_scale);
    cfg.cross_blink.apply(blink)
}

/// The per-question clock for the level in play, or `None` when the level is
/// untimed (`time_limit_frames == 0`). Armed fresh for each puzzle: the
/// countdown drives the draining time bar (colours and strip from config),
/// with the digital seconds readout if the layout places one
/// (`layout.timer_seconds_at`). The binding — bar fraction, readout re-bake,
/// fire-once expiry — is the reusable `ratgames::TimedGauge`.
fn question_gauge(ctx: &Ctx) -> Option<TimedGauge<Ctx>> {
    let frames = ctx.session.current_time_limit_frames();
    (frames > 0).then(|| {
        let gauge = TimedGauge::new(
            Countdown::new(frames),
            ctx.timer_bar.bar(ctx.layout.timer_bar),
        );
        match ctx.layout.timer_seconds_at {
            Some(at) => gauge.with_seconds(ctx.frames_per_second, move |secs, ctx: &Ctx| {
                ctx.hud_factory()
                    .at(&secs.to_string(), at, ctx.text.hud_scale)
            }),
            None => gauge,
        }
    })
}

/// The spelling half of the play screen: grade answers through the session,
/// build the feedback beat from config, tally the level, and route each
/// resolution. The phase machinery — the answer commit, the frozen clock, the
/// feedback freeze/skip, resolve-once — is the reusable
/// `ratgames::ChallengeScreen` this drives; the driver carries only the
/// per-level state.
struct WordChallenge {
    /// This driver plays one level; its name (for the Level Clear tally) and
    /// the hit / miss tally over the whole level (for its accuracy) live here.
    level_name: String,
    hits: u32,
    misses: u32,
}

impl WordChallenge {
    fn new(ctx: &Ctx) -> Self {
        Self {
            level_name: ctx.session.current_level_name().to_string(),
            hits: 0,
            misses: 0,
        }
    }

    /// The graded shape for an answer or a timeout: the beat (a miss opens
    /// with the flashing reject cross, a hit tints the screen with a fading
    /// success wash, both hold the verdict), the HUD re-baked so the new
    /// score / lives show behind it, and the pending route for when the beat
    /// ends.
    fn graded(&self, ctx: &Ctx, report: &AttemptReport, verdict: &str) -> GradedAttempt<Pending> {
        let cfg = ctx.feedback;
        let factory = ctx.banner_factory();
        let (reject, wash) = if report.correct {
            (None, Some(cfg.correct_color))
        } else {
            (
                Some(reject_cross(&cfg, &*ctx.glyphs, ctx.virtual_size)),
                None,
            )
        };
        GradedAttempt {
            beat: FeedbackBeat::new(
                reject,
                wash,
                factory.centered(verdict, ctx.text.banner_scale),
                Countdown::new(cfg.duration_frames),
            ),
            status: hud(
                &ctx.session,
                &ctx.hud_factory(),
                ctx.text.hud_scale,
                &ctx.copy.hud,
                ctx.layout.hud_at,
            ),
            pending: pending_for(report),
        }
    }
}

impl Challenge<Ctx> for WordChallenge {
    type Pending = Pending;

    fn view(&mut self, ctx: &Ctx) -> ChallengeView<Ctx> {
        let session = &ctx.session;
        // The masked word is display text (banner source); the HUD line is
        // body text (hud source). Typed answers use the shared input field,
        // so no choice list is ever offered.
        let factory = ctx.banner_factory();
        let body = ctx.hud_factory();
        ChallengeView {
            prompt: factory.centered(&session.current_prompt(), ctx.text.banner_scale),
            status: hud(
                session,
                &body,
                ctx.text.hud_scale,
                &ctx.copy.hud,
                ctx.layout.hud_at,
            ),
            choices: None,
            gauge: question_gauge(ctx),
        }
    }

    fn grade(
        &mut self,
        answer: ChallengeAnswer,
        time_left: Option<u32>,
        ctx: &mut Ctx,
    ) -> GradedAttempt<Pending> {
        // Typed answers only: the view never offers choices, so the screen
        // cannot deliver a pick — grade a stray one as an empty answer rather
        // than panic.
        let report = match answer {
            ChallengeAnswer::Typed(text) => ctx.session.submit_typed_answer(text),
            ChallengeAnswer::Choice(_) => ctx.session.submit_typed_answer(""),
        };
        // Tally this level's hits / misses for the Level Clear accuracy, and
        // reward a correct answer with a time bonus for the seconds to spare.
        if report.correct {
            self.hits += 1;
            let bonus = match time_left {
                Some(frames) if ctx.frames_per_second > 0 => {
                    (frames / ctx.frames_per_second) * ctx.time_bonus_per_second
                }
                _ => 0,
            };
            ctx.session.award_bonus(bonus);
        } else {
            self.misses += 1;
        }
        let verdict = verdict_line(&report, &ctx.copy.verdict);
        self.graded(ctx, &report, &verdict)
    }

    fn time_out(&mut self, ctx: &mut Ctx) -> GradedAttempt<Pending> {
        // Record the expired question as a miss (no answer) with a "TIME UP"
        // verdict — the same beat a wrong answer gets, so the run sequences
        // identically.
        let report = ctx.session.time_out();
        self.misses += 1;
        let time_up = ctx.copy.verdict.time_up.clone();
        self.graded(ctx, &report, &time_up)
    }

    fn resolve(&mut self, pending: Pending, ctx: &mut Ctx) -> ChallengeResolution<Ctx> {
        match pending {
            Pending::Advance => ChallengeResolution::Stay,
            Pending::LevelCleared => {
                ChallengeResolution::Leave(ScreenChange::Replace(level_clear_screen(
                    ctx,
                    &self.level_name,
                    ctx.session.run().score().points(),
                    self.hits,
                    self.misses,
                    ctx.interstitial.countdown(),
                )))
            }
            Pending::Finish(phase) => {
                // A game over with a continue to spend detours through the
                // CONTINUE? prompt — the run is not recorded yet, because a
                // continued run plays on. Every other ending records here.
                ChallengeResolution::Leave(
                    if phase == RunPhase::GameOver && ctx.session.can_continue() {
                        ScreenChange::Replace(continue_screen(ctx))
                    } else {
                        ctx.record_run();
                        ScreenChange::Replace(result_screen(ctx, phase))
                    },
                )
            }
        }
    }

    fn cancel(&mut self, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        ctx.quit = true;
        ScreenChange::None
    }
}

/// Play: the masked word as a centred banner, a score/lives HUD, and the
/// shared typed field for the missing letters. Enter grades the answer, then a
/// brief feedback beat flashes the verdict (and the whole word on a miss)
/// before the next puzzle or the result screen. The two-phase controller is
/// `ratgames::ChallengeScreen`; the app supplies the spelling driver.
fn play_screen(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    Box::new(ChallengeScreen::new(WordChallenge::new(ctx), ctx))
}

/// Level Intro card: a brief "ROUND N OF M" interstitial with the level's
/// theme name, difficulty, and target, shown before each level on a
/// [`TimedCard`]. It holds until the countdown expires then auto-advances into
/// play; Enter skips the wait, Esc quits. The banners are app-styled; the
/// hold + input mechanic is the reusable card.
fn level_intro_screen(ctx: &Ctx, countdown: Countdown) -> Box<dyn Screen<Ctx>> {
    let session = &ctx.session;
    let levels = session.run().levels();
    let round = levels.current() + 1;
    // Left-anchored hud-scale lines, like the HUD — body text bakes through
    // the hud source.
    let factory = ctx.hud_factory();
    let banners = BannerColumn::at_x(ctx.layout.screen_x)
        .lines(
            [
                fill_placeholders(
                    &ctx.copy.level_intro.round,
                    &[round.to_string(), levels.total().to_string()],
                ),
                session.current_level_name().to_string(),
                fill_placeholders(
                    &ctx.copy.level_intro.goal,
                    &[
                        session.current_difficulty().to_string(),
                        session.goal().required_successes().to_string(),
                    ],
                ),
            ],
            &ctx.layout.level_intro_ys,
            ctx.text.hud_scale,
        )
        .bake(&factory);
    // Confirm or expiry begins play for the now-current level (built fresh
    // from the context at exit time); cancel quits.
    Box::new(TimedCard::new(
        banners,
        countdown,
        |exit, ctx: &mut Ctx| match exit {
            TimedCardExit::Cancelled => {
                ctx.quit = true;
                ScreenChange::None
            }
            TimedCardExit::Confirmed | TimedCardExit::Expired => {
                ScreenChange::Replace(play_screen(ctx))
            }
        },
    ))
}

/// Level Clear card: the just-cleared level's tally — its name, the running
/// score, and this level's accuracy — on a [`TimedCard`]. It holds until the
/// countdown expires then auto-advances into the next level's intro; Enter
/// skips the wait, Esc quits.
fn level_clear_screen(
    ctx: &Ctx,
    level_name: &str,
    score: u32,
    hits: u32,
    misses: u32,
    countdown: Countdown,
) -> Box<dyn Screen<Ctx>> {
    // Tally lines are body text; they bake through the hud source.
    let factory = ctx.hud_factory();
    let banners = BannerColumn::at_x(ctx.layout.screen_x)
        .lines(
            [
                ctx.copy.level_clear.title.clone(),
                level_name.to_string(),
                fill_placeholders(&ctx.copy.level_clear.score, &[score.to_string()]),
                fill_placeholders(
                    &ctx.copy.level_clear.accuracy,
                    &[accuracy_percent(hits, misses).to_string()],
                ),
            ],
            &ctx.layout.level_clear_ys,
            ctx.text.hud_scale,
        )
        .bake(&factory);
    // Confirm or expiry moves on to the next level's intro (the run has
    // already advanced to it), built fresh from the context; cancel quits.
    Box::new(TimedCard::new(
        banners,
        countdown,
        |exit, ctx: &mut Ctx| match exit {
            TimedCardExit::Cancelled => {
                ctx.quit = true;
                ScreenChange::None
            }
            TimedCardExit::Confirmed | TimedCardExit::Expired => {
                ScreenChange::Replace(level_intro_screen(ctx, ctx.interstitial.countdown()))
            }
        },
    ))
}

/// The game-over CONTINUE? prompt: a centred banner and a live seconds readout
/// on the reusable `ratgames::ContinuePrompt` flow (Continued / Declined /
/// Cancelled). Enter spends a continue and resumes the run on its current
/// level (via that level's intro); letting the countdown run out declines and
/// moves on to the result. Esc still quits — the finished run is recorded on
/// both leaving paths, and NOT when it continues (it plays on).
fn continue_screen(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    // The CONTINUE? title (and the live seconds digits below) are display
    // text; the instruction line is body text.
    let banners = vec![
        ctx.banner_factory()
            .centered(&ctx.copy.continue_prompt.title, ctx.text.banner_scale),
        ctx.hud_factory().at(
            &fill_placeholders(
                &ctx.copy.continue_prompt.prompt,
                &[ctx.session.continues_remaining().to_string()],
            ),
            Point::new(ctx.layout.screen_x, ctx.layout.continue_prompt_y),
            ctx.text.hud_scale,
        ),
    ];
    Box::new(
        ContinuePrompt::new(
            banners,
            ctx.continue_prompt.countdown(),
            |exit, ctx: &mut Ctx| {
                match exit {
                    // Spend the continue and play on from the current level.
                    ContinueExit::Continued if ctx.session.continue_run() => {
                        ScreenChange::Replace(level_intro_screen(ctx, ctx.interstitial.countdown()))
                    }
                    // The offer lapsed, or a continue that could not be spent:
                    // the run is over — record it and show the result.
                    ContinueExit::Continued | ContinueExit::Declined => {
                        ctx.record_run();
                        ScreenChange::Replace(result_screen(ctx, RunPhase::GameOver))
                    }
                    // Esc quits, as everywhere — but the finished run still records.
                    ContinueExit::Cancelled => {
                        ctx.record_run();
                        ctx.quit = true;
                        ScreenChange::None
                    }
                }
            },
        )
        .with_seconds(ctx.frames_per_second, |secs, ctx: &Ctx| {
            ctx.banner_factory().at(
                &secs.to_string(),
                ctx.layout.continue_seconds_at,
                ctx.text.banner_scale,
            )
        }),
    )
}

/// The ending title for a finished run: the first rank the run earned, or the
/// plain phase title. Pure, so it is unit-tested directly.
fn ending_title<'a>(phase: RunPhase, rank: Option<&'a str>, result: &'a ResultCopy) -> &'a str {
    rank.unwrap_or_else(|| {
        if phase == RunPhase::Won {
            result.win.as_str()
        } else {
            result.game_over.as_str()
        }
    })
}

/// Result: the ending banner — the run's earned rank ("WORD WIZARD"), or the
/// plain win / game-over title — and the final score. Enter shows the board;
/// Esc quits. The static-card mechanism is `ratgames::PromptScreen`; the app
/// supplies the title / score banners and the routing.
fn result_screen(ctx: &Ctx, phase: RunPhase) -> Box<dyn Screen<Ctx>> {
    let rank = ctx.session.rank(&ctx.ranks);
    let title = ending_title(phase, rank, &ctx.copy.result);
    let score = fill_placeholders(
        &ctx.copy.result.score,
        &[ctx.session.run().score().points().to_string()],
    );
    // The ending title is display text; the score line is body text.
    let overlays = vec![
        ctx.banner_factory().centered(title, ctx.text.banner_scale),
        ctx.hud_factory()
            .at(&score, ctx.layout.result_score_at, ctx.text.hud_scale),
    ];
    Box::new(PromptScreen::new(
        overlays,
        |exit, ctx: &mut Ctx| match exit {
            PromptExit::Confirmed => ScreenChange::Replace(high_score_screen(ctx)),
            PromptExit::Cancelled => {
                ctx.quit = true;
                ScreenChange::None
            }
            // No idle trigger is armed on a result card.
            PromptExit::Idled => ScreenChange::None,
        },
    ))
}

/// Bake the ranked board in the app's layout: a "HIGH SCORES" header, the
/// entries (up to `capacity`) in two columns, and a "PRESS ENTER" footer — all
/// banners in the config text style, anchored to virtual-screen positions.
/// Shared by the post-run high-score screen and the attract loop.
fn baked_board(ctx: &Ctx) -> Vec<ShadowBanner> {
    // ratgames grid-places and bakes the ranked rows and footer through the
    // hud factory (body-height content); the header is a display-height line,
    // so it bakes through the banner factory — the scale↔factory rule. The app
    // supplies the layout (from config), its banner style, and the copy.
    let hud_factory = ctx.hud_factory();
    let board = HighScoreBoard::new(
        &ctx.scores,
        &hud_factory,
        HighScoreBoardSpec {
            layout: ctx.layout.board,
            capacity: ctx.capacity,
            row_scale: ctx.text.hud_scale,
            header: None,
            footer: Some(BoardFooter {
                text: ctx.copy.board.footer.as_str(),
                gap_below_rows: ctx.layout.board_footer_gap,
                scale: ctx.text.hud_scale,
            }),
        },
    );
    let mut banners = vec![ctx.banner_factory().at(
        &ctx.copy.board.header,
        ctx.layout.board_header_at,
        ctx.text.banner_scale,
    )];
    banners.extend(board.into_banners());
    banners
}

/// High scores: the ranked board shown after a run ends. Enter resets and
/// returns to the title; Esc quits. The static-card mechanism is
/// `ratgames::PromptScreen`; the app supplies the baked board and the routing.
fn high_score_screen(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    Box::new(PromptScreen::new(
        baked_board(ctx),
        |exit, ctx: &mut Ctx| match exit {
            PromptExit::Confirmed => {
                ctx.session.reset();
                ScreenChange::Replace(title_screen(ctx))
            }
            PromptExit::Cancelled => {
                ctx.quit = true;
                ScreenChange::None
            }
            // No idle trigger is armed on the board.
            PromptExit::Idled => ScreenChange::None,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratgames::{Bitmap8x8, LevelOutcome};

    /// The bundled product feedback config (from `style.json`), so the
    /// reject-cross test reads the shipped blink pattern rather than a
    /// duplicated Rust literal.
    fn cfg() -> FeedbackBeatConfig {
        crate::config::AppConfig::resolve(None)
            .expect("bundled config")
            .feedback
    }

    /// The bundled product copy (from `copy.json`), so string assertions read
    /// the real shipped text rather than the neutral `Default`.
    fn copy() -> CopyConfig {
        crate::config::AppConfig::resolve(None)
            .expect("bundled config")
            .copy
    }

    fn report(correct: bool, run_phase: RunPhase, revealed: Option<&str>) -> AttemptReport {
        AttemptReport {
            correct,
            level_outcome: LevelOutcome::InProgress,
            run_phase,
            revealed: revealed.map(str::to_string),
        }
    }

    #[test]
    fn a_correct_answer_reads_correct_and_advances() {
        assert_eq!(
            verdict_line(&report(true, RunPhase::Playing, None), &copy().verdict),
            "CORRECT"
        );
        assert_eq!(
            pending_for(&report(true, RunPhase::Playing, None)),
            Pending::Advance
        );
    }

    #[test]
    fn a_wrong_answer_reveals_the_whole_word_without_ambiguity() {
        // The verdict must state the word as THE WORD, not beside WRONG (which
        // would read as if that word were the wrong answer).
        let line = verdict_line(
            &report(false, RunPhase::Playing, Some("CAT")),
            &copy().verdict,
        );
        assert_eq!(line, "THE WORD WAS CAT");
    }

    #[test]
    fn a_miss_without_a_reveal_falls_back_to_a_bare_wrong() {
        // Timeouts grade no answer, so they reveal nothing.
        assert_eq!(
            verdict_line(&report(false, RunPhase::Playing, None), &copy().verdict),
            "WRONG"
        );
    }

    #[test]
    fn scaled_levels_scale_only_the_authored_time_limits() {
        use ratgames::LevelSpec;
        use wordgame_app::Words;

        let level = |frames: u32| WordLevel {
            name: "L".to_string(),
            difficulty: "EASY".to_string(),
            rules: LevelSpec {
                time_limit_frames: frames,
                ..LevelSpec::default()
            },
            content: Words {
                length_min: 3,
                length_max: 4,
                blanks: 1,
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
        let result = copy().result;
        assert_eq!(
            ending_title(RunPhase::Won, Some("NO MISS CHAMP"), &result),
            "NO MISS CHAMP"
        );
        assert_eq!(ending_title(RunPhase::Won, None, &result), "YOU WIN");
        assert_eq!(ending_title(RunPhase::GameOver, None, &result), "GAME OVER");
        // A rank on a lost run (a game may configure one) still shows.
        assert_eq!(
            ending_title(RunPhase::GameOver, Some("GOOD EFFORT"), &result),
            "GOOD EFFORT"
        );
    }

    #[test]
    fn a_finished_run_hands_off_to_the_result_screen() {
        // A won run ends on a correct answer, a game-over on a wrong one;
        // either way the beat hands off to the result screen instead of
        // advancing.
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
            revealed: None,
        };
        assert_eq!(pending_for(&clear_playing), Pending::LevelCleared);

        // A clear that also won the run goes to the result, not the tally.
        let clear_won = AttemptReport {
            correct: true,
            level_outcome: LevelOutcome::Cleared,
            run_phase: RunPhase::Won,
            revealed: None,
        };
        assert_eq!(pending_for(&clear_won), Pending::Finish(RunPhase::Won));

        // An in-progress answer (or a retry after a lost life) stays on the
        // level.
        assert_eq!(
            pending_for(&report(true, RunPhase::Playing, None)),
            Pending::Advance
        );
    }
}

//! Math challenge presentation, grading feedback, and play transitions.
use crate::config::VerdictCopy;
use mathgame_app::{AttemptReport, MathgameSession};
use ratgames::BannerContext;
use ratgames::{
    BannerAnchor, Blink, Challenge, ChallengeAnswer, ChallengeResolution, ChallengeScreen,
    ChallengeView, ChoiceList, Countdown, FeedbackBeat, FeedbackBeatConfig, GlyphSource,
    GradedAttempt, LevelOutcome, Point, RunPhase, Screen, ScreenChange, ShadowBanner,
    ShadowBannerFactory, Size, TimedGauge, fill_placeholders,
};

use super::context::Ctx;
use super::progress::{continue_screen, level_clear_screen};
use super::results::result_screen;

/// The top-of-screen score / lives / level line, anchored top-left. `template`
/// is the copy's HUD format — three `{}` (score, lives, level).
fn hud(
    session: &MathgameSession,
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
fn verdict_line(report: &AttemptReport, verdict: &VerdictCopy) -> String {
    if report.correct {
        verdict.correct.clone()
    } else {
        match report.evaluation.as_ref() {
            Some(evaluation) => fill_placeholders(
                &verdict.answer_is,
                &[evaluation.canonical_answer().to_fraction_string()],
            ),
            None => verdict.wrong.clone(),
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
fn reject_cross(cfg: &FeedbackBeatConfig, source: &dyn GlyphSource, virtual_size: Size) -> Blink {
    let cross = source.glyph('X').to_sprite(cfg.wrong_color);
    let blink = Blink::new(cross, BannerAnchor::Center, virtual_size).scale(cfg.cross_scale);
    cfg.cross_blink.apply(blink)
}

/// The multiple-choice list for the session's current problem — a left-anchored
/// pixel-art [`ChoiceList`], or `None` in typed mode (which uses the shared answer
/// field instead). The layout values stay app-side, like the high-score board's.
fn choices_for(
    session: &MathgameSession,
    factory: &ShadowBannerFactory,
    scale: u32,
    at: Point,
    row_pitch: i32,
) -> Option<ChoiceList> {
    let labels = session.current_choices()?;
    Some(ChoiceList::new(labels, at, row_pitch, scale, factory))
}

/// The equation banner, placed for the answer mode: centred (above the bottom
/// input field) in typed mode, or anchored near the top — clear of the choice
/// list below it — in multiple-choice mode.
fn equation_banner(
    session: &MathgameSession,
    factory: &ShadowBannerFactory,
    scale: u32,
    mc_at: Point,
) -> ShadowBanner {
    let prompt = session.current_prompt();
    if session.current_choices().is_some() {
        factory.at(&prompt, mc_at, scale)
    } else {
        factory.centered(&prompt, scale)
    }
}

/// The per-question clock for the level in play, or `None` when the level is
/// untimed (`time_limit_frames == 0`). Armed fresh for each problem: the
/// countdown drives the draining time bar (colours and strip from config), with
/// the digital seconds readout if the layout places one
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

/// The math half of the play screen: grade answers through the session, build
/// the feedback beat from config, tally the level, and route each resolution.
/// The phase machinery — the answer commit, the frozen clock, the feedback
/// freeze/skip, resolve-once — is the reusable `ratgames::ChallengeScreen` this
/// drives; the driver carries only the per-level state.
struct MathChallenge {
    /// This driver plays one level; its name (for the Level Clear tally) and
    /// the hit / miss tally over the whole level (for its accuracy) live here.
    level_name: String,
    hits: u32,
    misses: u32,
}

impl MathChallenge {
    fn new(ctx: &Ctx) -> Self {
        Self {
            level_name: ctx.session.current_level_name().to_string(),
            hits: 0,
            misses: 0,
        }
    }

    /// The graded shape for an answer or a timeout: the beat (a miss opens with
    /// the flashing reject cross, a hit tints the screen with a fading success
    /// wash, both hold the verdict), the HUD re-baked so the new score / lives
    /// show behind it, and the pending route for when the beat ends.
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

impl Challenge<Ctx> for MathChallenge {
    type Pending = Pending;

    fn view(&mut self, ctx: &Ctx) -> ChallengeView<Ctx> {
        let session = &ctx.session;
        // Display text bakes through the banner source; body text (the HUD
        // line, the choice rows) through the hud source.
        let factory = ctx.banner_factory();
        let body = ctx.hud_factory();
        ChallengeView {
            prompt: equation_banner(
                session,
                &factory,
                ctx.text.banner_scale,
                ctx.layout.equation_mc_at,
            ),
            status: hud(
                session,
                &body,
                ctx.text.hud_scale,
                &ctx.copy.hud,
                ctx.layout.hud_at,
            ),
            choices: choices_for(
                session,
                &body,
                ctx.text.hud_scale,
                ctx.layout.choices_at,
                ctx.layout.choices_row_pitch,
            ),
            gauge: question_gauge(ctx),
        }
    }

    fn grade(
        &mut self,
        answer: ChallengeAnswer,
        time_left: Option<u32>,
        ctx: &mut Ctx,
    ) -> GradedAttempt<Pending> {
        // Grade the picked choice in multiple-choice mode, else the typed
        // answer. Both produce the same report, so the beat is identical.
        let report = match answer {
            ChallengeAnswer::Choice(index) => ctx.session.submit_choice(index),
            ChallengeAnswer::Typed(text) => ctx.session.submit_typed_answer(text),
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

/// Play: the current equation as a banner, a score/lives HUD, and the answer —
/// either the shared typed field or a multiple-choice list. Enter grades the
/// answer, then a brief feedback beat flashes the verdict (and the correct answer
/// on a miss) before the next problem or the result screen. The two-phase
/// controller is `ratgames::ChallengeScreen`; the app supplies the math driver.
pub(super) fn play_screen(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    Box::new(ChallengeScreen::new(MathChallenge::new(ctx), ctx))
}

#[cfg(test)]
mod tests;

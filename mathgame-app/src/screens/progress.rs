//! Level intro, level clear, and continue interstitials.
use ratgames::BannerContext;
use ratgames::{
    BannerColumn, ContinueExit, ContinuePrompt, Countdown, Point, RunPhase, Screen, ScreenChange,
    TimedCard, TimedCardExit, accuracy_percent, fill_placeholders,
};

use super::context::Ctx;
use super::play::play_screen;
use super::results::result_screen;

/// Level Intro card: a brief "ROUND N OF M" interstitial with the level's theme
/// name, difficulty, and target, shown before each level on a [`TimedCard`]. It
/// holds until the countdown expires then auto-advances into play; Enter skips the
/// wait, Esc quits. The banners are app-styled; the hold + input mechanic is the
/// reusable card.
pub(super) fn level_intro_screen(ctx: &Ctx, countdown: Countdown) -> Box<dyn Screen<Ctx>> {
    let session = &ctx.session;
    let levels = session.run().levels();
    let round = levels.current() + 1;
    // Left-anchored hud-scale lines, like the HUD and choice list — a first cut the
    // visual pass can re-scale/reposition. Body text bakes through the hud source.
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
                ScreenChange::Replace(play_screen(ctx))
            }
        },
    ))
}

/// Level Clear card: the just-cleared level's tally — its name, the running score,
/// and this level's accuracy — on a [`TimedCard`]. It holds until the countdown
/// expires then auto-advances into the next level's intro; Enter skips the wait,
/// Esc quits.
pub(super) fn level_clear_screen(
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
                ScreenChange::Replace(level_intro_screen(ctx, ctx.interstitial.countdown()))
            }
        },
    ))
}

/// The game-over CONTINUE? prompt: a centred banner and a live seconds readout
/// on the reusable `ratgames::ContinuePrompt` flow (Continued / Declined /
/// Cancelled). Enter spends a continue and resumes the run on its current level
/// (via that level's intro); letting the countdown run out declines and moves on
/// to the result. Esc still quits — the finished run is recorded on both leaving
/// paths, and NOT when it continues (it plays on).
pub(super) fn continue_screen(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
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

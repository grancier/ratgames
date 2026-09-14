//! Run results and high-score presentation.
use crate::config::ResultCopy;
use ratgames::BannerContext;
use ratgames::{
    BoardFooter, HighScoreBoard, HighScoreBoardSpec, PromptExit, PromptScreen, RunPhase, Screen,
    ScreenChange, ShadowBanner, fill_placeholders,
};

use super::context::Ctx;
use super::menus::title_screen;

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

/// Result: the ending banner — the run's earned rank ("MATH MASTER"), or the
/// plain win / game-over title — and the final score. Enter shows the board.
/// The result screen for the run as it stands, ranked against the configured
/// endings — built from the context wherever a run finishes. Enter moves on to
/// the high-score board; Esc quits. The static-card mechanism is
/// `ratgames::PromptScreen`; the app supplies the title / score banners and the
/// routing.
pub(super) fn result_screen(ctx: &Ctx, phase: RunPhase) -> Box<dyn Screen<Ctx>> {
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
/// banners in the config text style, anchored to virtual-screen positions. Two
/// columns because at 32px a ten-row board is far taller than the 360px screen;
/// five per column fits comfortably. Shared by the post-run high-score screen
/// and the attract loop.
pub(super) fn baked_board(ctx: &Ctx) -> Vec<ShadowBanner> {
    // ratgames grid-places and bakes the ranked rows and footer through the hud
    // factory (body-height content); the header is a display-height line, so it
    // bakes through the banner factory — the scale↔factory rule. The app
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

/// High scores: the ranked board shown after a run ends. Enter resets and returns
/// to the title; Esc quits. The static-card mechanism is `ratgames::PromptScreen`;
/// the app supplies the baked board and the routing.
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
mod tests;

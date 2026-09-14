//! Title, attract loop, difficulty selection, and player naming.
use ratgames::BannerContext;
use ratgames::{
    AttractCard, AttractLoop, BannerColumn, ChoiceList, ChoiceScreen, Point, PromptExit,
    PromptScreen, Screen, ScreenChange, TextEntryExit, TextEntryScreen,
};

use super::context::Ctx;
use super::progress::level_intro_screen;
use super::results::baked_board;

/// Title screen: a banner. Enter starts, Esc quits — and left idle long enough,
/// it hands off to the attract loop (high scores, then how-to, cycling until any
/// key wakes it back here). The static-card mechanism (banners + one-shot
/// confirm/cancel routing + the resettable idle trigger) is
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
/// wakes the title. The cycling and rendering are `ratgames::AttractLoop`; the app
/// supplies the two cards and where waking leads.
fn attract_loop(ctx: &Ctx) -> Box<dyn Screen<Ctx>> {
    let scores = baked_board(ctx);

    // The card title is display text (banner source); the instruction lines are
    // body text (hud source).
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

/// Difficulty select: a caret menu over the config's presets. Arrows move, Enter
/// rebuilds the run for the chosen preset and moves on to name entry, Esc quits.
/// Shown only when at least one preset is configured. The menu mechanism (title +
/// caret list + navigation + routing) is `ratgames::ChoiceScreen`; the app supplies
/// the preset labels and what a choice does — scale and rebuild the run, then name
/// entry.
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

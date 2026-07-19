//! `high_score` — read a high-score table from JSON and show the ranked board.
//!
//! A showcase of the reusable high-score stack: [`JsonHighScoreStore`] loads a
//! [`HighScores`] table from a file, [`HighScoreBoard`] bakes it (header, ranked
//! rows, footer) into pixel-art [`ShadowBanner`]s through a
//! [`ShadowBannerFactory`], and a [`PromptScreen`] holds it until Enter/Esc — all
//! driven by [`MinifbHost::run`]. Only ratgames. Run with
//! `cargo run --example high_score --features minifb`.

use anyhow::Result;
use ratgames::{
    BoardFooter, BoardLine, Color, FontConfig, HighScoreBoard, HighScoreBoardSpec, HighScoreLayout,
    JsonHighScoreStore, MinifbHost, Point, Presentation, PromptExit, PromptScreen,
    RasterGlyphSource, ScreenChange, ScreenStack, ShadowBannerFactory, ShadowStyle, Size,
    SystemFont, WindowConfig,
};

const VIRTUAL: Size = Size { w: 640, h: 360 };
const BACKDROP: Color = Color::rgb(0x10, 0x12, 0x28);
/// The board file, resolved at compile time so the example runs from any cwd.
const SCORES_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/high_scores.json");

/// The one durable bit of state the host loop watches.
#[derive(Default)]
struct Ctx {
    quit: bool,
}

fn main() -> Result<()> {
    // Load the ranked table from disk (a plain JSON array of {name, points}).
    let scores = JsonHighScoreStore::new(SCORES_PATH).load()?;

    // Bake the board — header, ranked rows, footer — through a raster source.
    let source = RasterGlyphSource::new(SystemFont::load(&FontConfig::default())?, 24);
    let factory = ShadowBannerFactory::new(&source, ShadowStyle::default(), VIRTUAL);
    let banners = HighScoreBoard::new(
        &scores,
        &factory,
        HighScoreBoardSpec {
            layout: HighScoreLayout {
                origin: Point::new(210, 96),
                row_pitch: 34,
                column_width: 300,
                rows_per_column: 8,
                name_width: 6,
            },
            capacity: 8,
            row_scale: 1,
            header: Some(BoardLine {
                text: "HIGH SCORES",
                at: Point::new(210, 36),
                scale: 2,
            }),
            footer: Some(BoardFooter {
                text: "PRESS ENTER",
                gap_below_rows: 24,
                scale: 1,
            }),
        },
    )
    .into_banners();

    // Hold the board until the player confirms or cancels; either way, quit.
    let board = PromptScreen::new(banners, |exit, ctx: &mut Ctx| {
        match exit {
            PromptExit::Confirmed | PromptExit::Cancelled => ctx.quit = true,
            PromptExit::Idled => {}
        }
        ScreenChange::None
    });

    let window = WindowConfig {
        title: "ratgames: high scores".to_string(),
        width: Some(VIRTUAL.w * 2),
        height: Some(VIRTUAL.h * 2),
        ..WindowConfig::default()
    };
    let presentation = Presentation::new(VIRTUAL, BACKDROP, Color::rgb(0, 0, 0), 1);
    let mut host = MinifbHost::new(&window, presentation)?;
    let mut stack = ScreenStack::new(Box::new(board));
    let mut ctx = Ctx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}

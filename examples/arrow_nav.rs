//! `arrow_nav` — steer a block around the screen with the arrow keys.
//!
//! A minimal showcase of `ratgames::tiles`: a [`TileMarker`] on a [`TileCursor`]
//! moves one [`TileGrid`] cell per keydown and holds at the grid's edge — the
//! same positioning mazegame's player uses. Only ratgames, no font. Run with
//! `cargo run --example arrow_nav --features minifb`; arrows move, Esc quits.

use anyhow::Result;
use ratgames::{
    Color, Direction, MinifbHost, OverlayLayer, PixelLayer, Point, Presentation, Screen,
    ScreenChange, ScreenStack, Size, TileCursor, TileGrid, TileMarker, UiInput, WindowConfig,
};

const VIRTUAL: Size = Size { w: 640, h: 360 };
const TILE: u32 = 40;
const BACKDROP: Color = Color::rgb(0x10, 0x12, 0x28);
const BLOCK: Color = Color::rgb(0xF2, 0xC9, 0x4C);

/// The one durable bit of state the host loop watches.
#[derive(Default)]
struct Ctx {
    quit: bool,
}

struct NavScreen {
    block: TileMarker,
}

impl Screen<Ctx> for NavScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        let dir = match input {
            UiInput::Left => Direction::West,
            UiInput::Right => Direction::East,
            UiInput::Up => Direction::North,
            UiInput::Down => Direction::South,
            UiInput::Cancel => {
                ctx.quit = true;
                return ScreenChange::None;
            }
            _ => return ScreenChange::None,
        };
        // The cursor holds in place at the grid's edge — an edge is a wall here.
        self.block.cursor_mut().step(dir);
        ScreenChange::None
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        world: &mut Vec<&'a dyn PixelLayer>,
        _overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        world.push(&self.block);
    }
}

fn main() -> Result<()> {
    let grid_cells = Size::new(VIRTUAL.w / TILE, VIRTUAL.h / TILE); // 16 x 9
    let grid = TileGrid::new(Point::ORIGIN, Size::new(TILE, TILE), grid_cells);
    let block = TileMarker::new(
        TileCursor::new(grid, grid_cells.w / 2, grid_cells.h / 2),
        BLOCK,
    );

    let window = WindowConfig {
        title: "ratgames: arrow nav".to_string(),
        width: Some(VIRTUAL.w),
        height: Some(VIRTUAL.h),
        ..WindowConfig::default()
    };
    let presentation = Presentation::new(VIRTUAL, BACKDROP, Color::rgb(0, 0, 0), 1);
    let mut host = MinifbHost::new(&window, presentation)?;
    let mut stack = ScreenStack::new(Box::new(NavScreen { block }));
    let mut ctx = Ctx::default();

    host.run(&mut stack, &mut ctx, |ctx| ctx.quit)?;
    Ok(())
}

//! `arrow_nav` — steer a block around the screen with the arrow keys.
//!
//! A minimal showcase of `ratgames::tiles`: a [`TileGrid`] maps a `(col, row)`
//! cell to the virtual-pixel rect it fills, and the block moves one cell per
//! keydown, clamped to the grid — the same positioning mazegame's player uses.
//! Only ratgames, no font. Run with
//! `cargo run --example arrow_nav --features minifb`; arrows move, Esc quits.

use anyhow::Result;
use ratgames::{
    Color, MinifbHost, OverlayLayer, PixelLayer, Point, Presentation, Screen, ScreenChange,
    ScreenStack, Size, Surface, TileGrid, UiInput, WindowConfig,
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

/// A solid block occupying one cell of a [`TileGrid`], drawn as a filled tile.
struct Block {
    grid: TileGrid,
    col: u32,
    row: u32,
}

impl PixelLayer for Block {
    fn render(&self, screen: &mut Surface) {
        screen.fill_rect(self.grid.tile_rect(self.col, self.row), BLOCK);
    }
}

/// One cell of movement in `(dx, dy)`, clamped so the block never leaves the
/// grid. Pure, so it is unit-tested below.
fn stepped(col: u32, row: u32, dx: i32, dy: i32, grid: Size) -> (u32, u32) {
    let c = (col as i32 + dx).clamp(0, grid.w as i32 - 1) as u32;
    let r = (row as i32 + dy).clamp(0, grid.h as i32 - 1) as u32;
    (c, r)
}

struct NavScreen {
    block: Block,
}

impl Screen<Ctx> for NavScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        let (dx, dy) = match input {
            UiInput::Left => (-1, 0),
            UiInput::Right => (1, 0),
            UiInput::Up => (0, -1),
            UiInput::Down => (0, 1),
            UiInput::Cancel => {
                ctx.quit = true;
                return ScreenChange::None;
            }
            _ => return ScreenChange::None,
        };
        let (col, row) = stepped(
            self.block.col,
            self.block.row,
            dx,
            dy,
            self.block.grid.size(),
        );
        self.block.col = col;
        self.block.row = row;
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
    let block = Block {
        grid,
        col: grid_cells.w / 2,
        row: grid_cells.h / 2,
    };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_move_one_cell_and_clamp_at_the_edges() {
        let grid = Size::new(4, 3);
        assert_eq!(stepped(1, 1, 1, 0, grid), (2, 1), "right one cell");
        assert_eq!(stepped(1, 1, 0, -1, grid), (1, 0), "up one cell");
        // The edges hold the block in.
        assert_eq!(stepped(0, 0, -1, 0, grid), (0, 0), "left edge clamps");
        assert_eq!(stepped(3, 2, 1, 1, grid), (3, 2), "bottom-right clamps");
    }
}

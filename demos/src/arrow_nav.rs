//! `arrow_nav` — steer a block around the screen with the arrow keys.
//!
//! A minimal showcase of `ratgames::tiles`: a [`TileMarker`] on a [`TileCursor`]
//! moves one [`TileGrid`] cell per keydown and holds at the grid's edge — the
//! same positioning mazegame's player uses. Only ratgames, no font.

use ratgames::{
    Color, Direction, OverlayLayer, PixelLayer, Point, Presentation, Screen, ScreenChange, Size,
    TileCursor, TileGrid, TileMarker, UiInput,
};

use crate::DemoCtx;

/// The virtual screen the demo composes into; a host integer-upscales it.
pub const VIRTUAL: Size = Size { w: 640, h: 360 };
const TILE: u32 = 40;
const BACKDROP: Color = Color::rgb(0x10, 0x12, 0x28);
const BLOCK: Color = Color::rgb(0xF2, 0xC9, 0x4C);

/// The compositor for the demo's fixed virtual screen — identical on every
/// host.
#[must_use]
pub fn presentation() -> Presentation {
    Presentation::new(VIRTUAL, BACKDROP, Color::rgb(0, 0, 0), 1)
}

/// The whole demo as one screen: the block, stepped a cell per arrow key.
pub struct NavScreen {
    block: TileMarker,
}

/// The demo, its block starting centre-grid.
#[must_use]
pub fn build() -> NavScreen {
    let grid_cells = Size::new(VIRTUAL.w / TILE, VIRTUAL.h / TILE); // 16 x 9
    let grid = TileGrid::new(Point::ORIGIN, Size::new(TILE, TILE), grid_cells);
    NavScreen {
        block: TileMarker::new(
            TileCursor::new(grid, grid_cells.w / 2, grid_cells.h / 2),
            BLOCK,
        ),
    }
}

impl Screen<DemoCtx> for NavScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut DemoCtx) -> ScreenChange<DemoCtx> {
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
        _ctx: &'a DemoCtx,
        world: &mut Vec<&'a dyn PixelLayer>,
        _overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        world.push(&self.block);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_quits_and_the_screen_contributes_one_world_layer() {
        let mut screen = build();
        let mut ctx = DemoCtx::default();
        screen.handle(UiInput::Cancel, &mut ctx);
        assert!(ctx.quit);

        let mut world: Vec<&dyn PixelLayer> = Vec::new();
        let mut overlays: Vec<&dyn OverlayLayer> = Vec::new();
        screen.collect_layers(&ctx, &mut world, &mut overlays);
        assert_eq!(world.len(), 1);
        assert!(overlays.is_empty());
    }
}

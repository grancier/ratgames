//! `room_scroll` — steer a block between rooms; each room is a lettered screen
//! and stepping off an edge scrolls to the neighbour.
//!
//! A showcase of the reusable scene machinery: an [`Overworld`] over a
//! [`RoomMap`] of four rooms, rendered by [`RoomView`] (the Zelda-style
//! screen-to-screen slide), with a block on a [`TileGrid`] and a big [`BigText`]
//! letter ([`Placard`]) per room. Stepping the block off an edge calls
//! [`Overworld::go`] and re-enters it on the opposite side of the new room.
//!
//! Because [`RoomView`] borrows the [`Overworld`] each frame, this drives the
//! host's primitives ([`MinifbHost::is_open`] / [`poll_inputs`](MinifbHost::poll_inputs)
//! / [`render`](MinifbHost::render)) directly rather than a `ScreenStack` — the
//! escape hatch the host documents for per-frame work. Only ratgames. Run with
//! `cargo run --example room_scroll --features minifb`; arrows move, Esc quits.

use anyhow::Result;
use ratgames::{
    BigText, Bitmap8x8, Color, Direction, MinifbHost, PixelLayer, Placard, Point, Presentation,
    Room, RoomId, RoomMap, Size, Surface, TextColors, TileGrid, UiInput, WindowConfig, palette,
};

const VIRTUAL: Size = Size { w: 640, h: 360 };
const TILE: u32 = 40;
const BLOCK: Color = Color::rgb(0xF2, 0xC9, 0x4C);
/// Frames a room-to-room slide takes.
const SLIDE_FRAMES: u16 = 12;

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

/// Where the block re-enters the new room after crossing an edge `dir`: the
/// opposite edge, same lane. Pure, so it is unit-tested below. (`y` grows
/// downward: going North enters from the bottom.)
fn reentry(dir: Direction, col: u32, row: u32, grid: Size) -> (u32, u32) {
    match dir {
        Direction::East => (0, row),
        Direction::West => (grid.w - 1, row),
        Direction::South => (col, 0),
        Direction::North => (col, grid.h - 1),
    }
}

/// Step the block one cell; if that leaves the grid, try to cross to the
/// neighbour that way and re-enter on the far edge. A crossing that has no
/// neighbour (the world's edge) is a no-op.
fn try_move(input: UiInput, block: &mut Block, world: &mut ratgames::Overworld) {
    let dir = match input {
        UiInput::Up => Direction::North,
        UiInput::Down => Direction::South,
        UiInput::Left => Direction::West,
        UiInput::Right => Direction::East,
        _ => return,
    };
    let grid = block.grid.size();
    let delta = dir.delta();
    let (nc, nr) = (block.col as i32 + delta.x, block.row as i32 + delta.y);
    if (0..grid.w as i32).contains(&nc) && (0..grid.h as i32).contains(&nr) {
        block.col = nc as u32;
        block.row = nr as u32;
    } else if world.go(dir) {
        let (col, row) = reentry(dir, block.col, block.row, grid);
        block.col = col;
        block.row = row;
    }
}

/// A big letter centred as a [`Placard`], baked chunky through the 8×8 bitmap
/// (a scene marker, not body text — so no font is needed).
fn letter(ch: char) -> Placard {
    let sprite = BigText::new(8)
        .outline(1)
        .shadow_depth(2)
        .colors(TextColors {
            fill: palette::FILL,
            outline: palette::OUTLINE,
            shadow: palette::SHADOW,
        })
        .build_with(&Bitmap8x8, &ch.to_string());
    Placard::new(sprite)
}

fn main() -> Result<()> {
    // A 2x2 world: A B over C D, each a distinct backdrop with its own letter.
    let ids = [
        RoomId::new(0),
        RoomId::new(1),
        RoomId::new(2),
        RoomId::new(3),
    ];
    let backdrops = [
        Color::rgb(0x24, 0x1a, 0x3a),
        Color::rgb(0x1a, 0x2e, 0x3a),
        Color::rgb(0x3a, 0x24, 0x1a),
        Color::rgb(0x1a, 0x3a, 0x24),
    ];
    let mut map = RoomMap::new();
    for (id, backdrop) in ids.iter().zip(backdrops) {
        map.insert(Room::new(*id, VIRTUAL, backdrop));
    }
    map.connect(ids[0], Direction::East, ids[1]); // A -> B
    map.connect(ids[0], Direction::South, ids[2]); // A -> C
    map.connect(ids[1], Direction::South, ids[3]); // B -> D
    map.connect(ids[2], Direction::East, ids[3]); // C -> D
    let mut world = ratgames::Overworld::new(map, ids[0], SLIDE_FRAMES).expect("start room exists");

    let letters: Vec<(RoomId, Placard)> = ids
        .iter()
        .zip(['A', 'B', 'C', 'D'])
        .map(|(id, ch)| (*id, letter(ch)))
        .collect();

    let grid_cells = Size::new(VIRTUAL.w / TILE, VIRTUAL.h / TILE); // 16 x 9
    let mut block = Block {
        grid: TileGrid::new(Point::ORIGIN, Size::new(TILE, TILE), grid_cells),
        col: grid_cells.w / 2,
        row: grid_cells.h / 2,
    };

    let window = WindowConfig {
        title: "ratgames: room scroll".to_string(),
        width: Some(VIRTUAL.w),
        height: Some(VIRTUAL.h),
        ..WindowConfig::default()
    };
    let presentation = Presentation::new(VIRTUAL, Color::rgb(0, 0, 0), Color::rgb(0, 0, 0), 1);
    let mut host = MinifbHost::new(&window, presentation)?;

    let mut quit = false;
    while host.is_open() && !quit {
        for input in host.poll_inputs() {
            match input {
                UiInput::Cancel => quit = true,
                // Ignore steering while a slide is in flight.
                _ if world.transition().is_settled() => try_move(input, &mut block, &mut world),
                _ => {}
            }
        }
        world.advance(); // drive any in-progress slide

        // The sliding rooms are the backdrop; while settled, the current room's
        // letter and the block sit on top (a slide shows just the scroll).
        let view = world.view();
        let mut layers: Vec<&dyn PixelLayer> = vec![&view];
        if world.transition().is_settled() {
            if let Some((_, placard)) = letters.iter().find(|(id, _)| *id == world.current()) {
                layers.push(placard);
            }
            layers.push(&block);
        }
        host.render(&layers, &[])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reentry_lands_on_the_opposite_edge() {
        let grid = Size::new(16, 9);
        assert_eq!(
            reentry(Direction::East, 15, 4, grid),
            (0, 4),
            "in from the left"
        );
        assert_eq!(
            reentry(Direction::West, 0, 4, grid),
            (15, 4),
            "in from the right"
        );
        assert_eq!(
            reentry(Direction::South, 7, 8, grid),
            (7, 0),
            "in from the top"
        );
        assert_eq!(
            reentry(Direction::North, 7, 0, grid),
            (7, 8),
            "in from the bottom"
        );
    }
}

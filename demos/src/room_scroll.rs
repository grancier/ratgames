//! `room_scroll` — steer a block between rooms; each room is a lettered screen
//! and stepping off an edge scrolls to the neighbour.
//!
//! A showcase of the reusable scene machinery: an [`Overworld`] over a
//! [`RoomMap`] of four rooms, rendered by [`RoomView`](ratgames::RoomView) (the
//! Zelda-style screen-to-screen slide), with a [`TileMarker`] block on a
//! [`TileGrid`] and a big [`BigText`] letter ([`Placard`]) per room. Stepping
//! the block off an edge ([`CursorStep::AtEdge`]) calls [`Overworld::go`] and
//! the cursor [`reenters`](ratgames::TileCursor::reenter) on the opposite side
//! of the new room.
//!
//! Because the room view borrows the [`Overworld`] each frame, this demo is
//! **not** a [`Screen`](ratgames::Screen): a host drives its primitives
//! directly — [`steer`](RoomScrollDemo::steer) per input,
//! [`advance`](RoomScrollDemo::advance) per frame, and
//! [`with_layers`](RoomScrollDemo::with_layers) to composite (the borrow stays
//! inside the call). This is the escape hatch the hosts document for per-frame
//! work; only ratgames.

use ratgames::{
    BigText, Bitmap8x8, Color, CursorStep, Direction, Overworld, PixelLayer, Placard, Point,
    Presentation, Room, RoomId, RoomMap, Size, TextColors, TileCursor, TileGrid, TileMarker,
    UiInput, palette,
};

/// The virtual screen the demo composes into; a host integer-upscales it.
pub const VIRTUAL: Size = Size { w: 640, h: 360 };
const TILE: u32 = 40;
const BLOCK: Color = Color::rgb(0xF2, 0xC9, 0x4C);
/// Frames a room-to-room slide takes.
const SLIDE_FRAMES: u16 = 12;

/// The compositor for the demo's fixed virtual screen.
#[must_use]
pub fn presentation() -> Presentation {
    Presentation::new(VIRTUAL, Color::rgb(0, 0, 0), Color::rgb(0, 0, 0), 1)
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

/// The whole demo: the 2×2 lettered world, the block steering through it, and
/// the slide state. A host drives it directly (see the module docs).
pub struct RoomScrollDemo {
    world: Overworld,
    letters: Vec<(RoomId, Placard)>,
    block: TileMarker,
}

impl RoomScrollDemo {
    /// The demo: rooms A B over C D, each with its own backdrop and letter,
    /// and the block starting centre-grid in room A.
    #[must_use]
    pub fn new() -> Self {
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
        let world = Overworld::new(map, ids[0], SLIDE_FRAMES).expect("start room exists");

        let letters = ids
            .iter()
            .zip(['A', 'B', 'C', 'D'])
            .map(|(id, ch)| (*id, letter(ch)))
            .collect();

        let grid_cells = Size::new(VIRTUAL.w / TILE, VIRTUAL.h / TILE); // 16 x 9
        let grid = TileGrid::new(Point::ORIGIN, Size::new(TILE, TILE), grid_cells);
        Self {
            world,
            letters,
            block: TileMarker::new(
                TileCursor::new(grid, grid_cells.w / 2, grid_cells.h / 2),
                BLOCK,
            ),
        }
    }

    /// Steer the block one cell for a directional input; at the grid's edge,
    /// cross to the neighbour that way and re-enter on the far edge. Ignored
    /// while a slide is in flight (and for non-directional input); a crossing
    /// with no neighbour (the world's edge) is a no-op.
    pub fn steer(&mut self, input: UiInput) {
        if !self.world.transition().is_settled() {
            return;
        }
        let dir = match input {
            UiInput::Up => Direction::North,
            UiInput::Down => Direction::South,
            UiInput::Left => Direction::West,
            UiInput::Right => Direction::East,
            _ => return,
        };
        if self.block.cursor_mut().step(dir) == CursorStep::AtEdge && self.world.go(dir) {
            self.block.cursor_mut().reenter(dir);
        }
    }

    /// Advance one frame: drive any in-progress slide.
    pub fn advance(&mut self) {
        self.world.advance();
    }

    /// Composite this frame's layers and hand them to `f` (typically a host's
    /// `render`). The sliding rooms are the backdrop; while settled, the
    /// current room's letter and the block sit on top (a slide shows just the
    /// scroll). The room view borrows the world, so the slice lives only for
    /// the call.
    pub fn with_layers<R>(&self, f: impl FnOnce(&[&dyn PixelLayer]) -> R) -> R {
        let view = self.world.view();
        let mut layers: Vec<&dyn PixelLayer> = vec![&view];
        if self.world.transition().is_settled() {
            if let Some((_, placard)) = self
                .letters
                .iter()
                .find(|(id, _)| *id == self.world.current())
            {
                layers.push(placard);
            }
            layers.push(&self.block);
        }
        f(&layers)
    }
}

impl Default for RoomScrollDemo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settled_frames_composite_rooms_letter_and_block() {
        let demo = RoomScrollDemo::new();
        let count = demo.with_layers(|layers| layers.len());
        assert_eq!(count, 3, "room view, letter, block");
    }

    #[test]
    fn crossing_an_edge_starts_a_slide_that_hides_the_actors() {
        let mut demo = RoomScrollDemo::new();
        // Walk the block to room A's east edge, then across into room B.
        for _ in 0..16 {
            demo.steer(UiInput::Right);
        }
        assert!(
            !demo.world.transition().is_settled(),
            "the crossing starts a slide"
        );
        let count = demo.with_layers(|layers| layers.len());
        assert_eq!(count, 1, "a slide shows just the scroll");
        // The slide finishes and the actors return.
        for _ in 0..32 {
            demo.advance();
        }
        assert_eq!(demo.with_layers(|layers| layers.len()), 3);
    }
}

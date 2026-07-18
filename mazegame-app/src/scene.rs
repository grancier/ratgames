//! [`MazeScene`] — the maze as a [`PixelLayer`], drawn in virtual pixels.
//!
//! Every maze tile renders as a `tile_px`-square: the bars are filled wall
//! tiles (so a 10px tile makes 10px-wide bars), the player is a filled block
//! on its tile, the exit tile is a coloured door (locked / open), and each
//! waiting number is its `Bitmap8x8` digit glyph centred in its tile — chunky
//! 8-bit digits that need no font, so rendering stays fully deterministic.
//!
//! The scene is a *snapshot* of the game, not a live borrow: layers are
//! collected by reference each frame, so the screen re-[`sync`](MazeScene::sync)s
//! the snapshot after every step (cheap at maze scale) rather than holding a
//! self-referential view.

use mazegame_core::MazeGame;
use ratgames::{Bitmap8x8, Color, GlyphSource, PixelLayer, Point, Rect, Size, Sprite, Surface};

use crate::config::{SceneColors, SceneConfig};

/// A drawable snapshot of one [`MazeGame`] frame. Build with
/// [`new`](Self::new), refresh with [`sync`](Self::sync), and push into the
/// world layers each frame.
pub struct MazeScene {
    tile_px: u32,
    origin: Point,
    colors: SceneColors,
    grid_w: usize,
    grid_h: usize,
    /// Row-major wall flags for the whole grid.
    walls: Vec<bool>,
    /// The player's tile.
    player: Point,
    /// The exit tile and whether it is open.
    exit: Point,
    exit_open: bool,
    /// Each waiting number: its baked digit sprite at its virtual position.
    digits: Vec<(Point, Sprite)>,
}

impl MazeScene {
    /// A scene of `game` laid out per `config`.
    #[must_use]
    pub fn new(config: &SceneConfig, game: &MazeGame) -> Self {
        let mut scene = Self {
            tile_px: config.tile_px,
            origin: config.origin,
            colors: config.colors,
            grid_w: 0,
            grid_h: 0,
            walls: Vec::new(),
            player: Point::ORIGIN,
            exit: Point::ORIGIN,
            exit_open: false,
            digits: Vec::new(),
        };
        scene.sync(game);
        scene
    }

    /// Re-snapshot `game` — walls, player, exit state, and the remaining
    /// digits. Full rebuild every time: at maze scale that is a few hundred
    /// flags and a handful of 8×8 glyph bakes, and it cannot drift.
    pub fn sync(&mut self, game: &MazeGame) {
        let maze = game.maze();
        self.grid_w = maze.width();
        self.grid_h = maze.height();
        self.walls = (0..self.grid_h as i32)
            .flat_map(|y| (0..self.grid_w as i32).map(move |x| (x, y)))
            .map(|(x, y)| maze.is_wall(mazegame_core::Tile::new(x, y)))
            .collect();
        self.player = Point::new(game.player().x, game.player().y);
        self.exit = Point::new(game.exit().x, game.exit().y);
        self.exit_open = game.exit_open();
        self.digits = game
            .collectibles()
            .iter()
            .map(|c| {
                let glyph = char::from_digit(u32::from(c.value), 10)
                    .expect("collectible values are single digits by construction");
                let sprite = Bitmap8x8.glyph(glyph).to_sprite(self.colors.digit);
                (
                    self.centered_in_tile(Point::new(c.at.x, c.at.y), sprite.size()),
                    sprite,
                )
            })
            .collect();
    }

    /// The virtual-pixel rect of one tile.
    fn tile_rect(&self, tile: Point) -> Rect {
        Rect::new(
            Point::new(
                self.origin.x + tile.x * self.tile_px as i32,
                self.origin.y + tile.y * self.tile_px as i32,
            ),
            Size::new(self.tile_px, self.tile_px),
        )
    }

    /// Where a sprite of `size` sits centred within `tile`.
    fn centered_in_tile(&self, tile: Point, size: Size) -> Point {
        let rect = self.tile_rect(tile);
        Point::new(
            rect.origin.x + (self.tile_px as i32 - size.w as i32) / 2,
            rect.origin.y + (self.tile_px as i32 - size.h as i32) / 2,
        )
    }

    fn door_color(&self) -> Color {
        if self.exit_open {
            self.colors.exit_open
        } else {
            self.colors.exit_locked
        }
    }
}

impl PixelLayer for MazeScene {
    fn render(&self, screen: &mut Surface) {
        for y in 0..self.grid_h {
            for x in 0..self.grid_w {
                if self.walls[y * self.grid_w + x] {
                    screen.fill_rect(
                        self.tile_rect(Point::new(x as i32, y as i32)),
                        self.colors.wall,
                    );
                }
            }
        }
        screen.fill_rect(self.tile_rect(self.exit), self.door_color());
        for (at, sprite) in &self.digits {
            screen.draw_sprite(sprite, *at);
        }
        // The block draws last, so it reads on top of the door when standing
        // on it.
        screen.fill_rect(self.tile_rect(self.player), self.colors.player);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazegame_core::{Direction, Maze, Tile};
    use ratgames::Presentation;

    /// A corridor game with one number in the middle: `#####` / `#.7.#`.
    fn corridor_game() -> MazeGame {
        let maze = Maze::from_rows(&["#####", "#...#", "#####"]).expect("map");
        MazeGame::with_maze(
            maze,
            Tile::new(1, 1),
            Tile::new(3, 1),
            vec![(Tile::new(2, 1), 7)],
        )
        .expect("valid game")
    }

    /// A 10px-tile scene at the origin, with each element a distinct colour.
    fn scene_config() -> SceneConfig {
        SceneConfig {
            tile_px: 10,
            origin: Point::ORIGIN,
            hud_at: Point::ORIGIN,
            colors: SceneColors {
                wall: Color::rgb(0, 0, 255),
                player: Color::rgb(255, 255, 0),
                digit: Color::rgb(255, 255, 255),
                exit_locked: Color::rgb(255, 0, 0),
                exit_open: Color::rgb(0, 255, 0),
            },
        }
    }

    fn pixel(surface: &Surface, x: u32, y: u32) -> u32 {
        surface.as_slice()[(y * surface.size().w + x) as usize]
    }

    /// Whether any pixel of `tile`'s 10px square is `want`.
    fn tile_holds(surface: &Surface, tile: (u32, u32), want: Color) -> bool {
        (0..10).any(|dy| {
            (0..10).any(|dx| pixel(surface, tile.0 * 10 + dx, tile.1 * 10 + dy) == want.packed())
        })
    }

    #[test]
    fn renders_bars_block_digit_and_locked_door() {
        let game = corridor_game();
        let config = scene_config();
        let scene = MazeScene::new(&config, &game);
        let mut screen = Surface::new(Size::new(50, 30), Color::rgb(0, 0, 0));

        scene.render(&mut screen);

        // A wall tile is solid bar colour, corner to corner (10px bars).
        assert_eq!(pixel(&screen, 0, 0), config.colors.wall.packed());
        assert_eq!(pixel(&screen, 9, 9), config.colors.wall.packed());
        // The block fills its 10px tile.
        assert_eq!(pixel(&screen, 10, 10), config.colors.player.packed());
        assert_eq!(pixel(&screen, 19, 19), config.colors.player.packed());
        // The digit's ink shows inside its tile; the door is locked red.
        assert!(tile_holds(&screen, (2, 1), config.colors.digit));
        assert!(tile_holds(&screen, (3, 1), config.colors.exit_locked));
        assert!(!tile_holds(&screen, (3, 1), config.colors.exit_open));
    }

    #[test]
    fn the_door_turns_open_after_the_last_number_is_collected() {
        let mut game = corridor_game();
        let config = scene_config();
        let mut scene = MazeScene::new(&config, &game);

        assert_eq!(game.step(Direction::Right).collected, Some(7));
        scene.sync(&game);

        let mut screen = Surface::new(Size::new(50, 30), Color::rgb(0, 0, 0));
        scene.render(&mut screen);
        // The digit is gone (the block covers its old tile), the door is green.
        assert!(tile_holds(&screen, (3, 1), config.colors.exit_open));
        assert!(!tile_holds(&screen, (3, 1), config.colors.exit_locked));
        assert_eq!(pixel(&screen, 25, 15), config.colors.player.packed());
    }

    #[test]
    fn a_frame_composites_through_the_presentation() {
        // The scene is a PixelLayer end to end: composite one frame through
        // Presentation into a window-sized framebuffer and find the upscaled
        // bar colour where the top-left wall tile lands.
        let game = corridor_game();
        let config = scene_config();
        let scene = MazeScene::new(&config, &game);
        let virtual_size = Size::new(50, 30);
        let mut presentation =
            Presentation::new(virtual_size, Color::rgb(0, 0, 0), Color::rgb(0, 0, 0), 1);
        let mut framebuffer = Surface::new(Size::new(100, 60), Color::rgb(0, 0, 0));

        presentation.render(&[&scene], &[], &mut framebuffer);

        assert_eq!(
            framebuffer.as_slice()[0],
            config.colors.wall.packed(),
            "the 2x upscale puts the first wall pixel at the framebuffer origin"
        );
    }
}

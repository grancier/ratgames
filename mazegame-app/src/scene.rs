//! [`MazeScene`] — the maze as a [`PixelLayer`], drawn in virtual pixels.
//!
//! Every maze tile renders as a `tile_px`-square: the bars are filled wall
//! tiles (so a 20px tile makes 20px-wide bars), the player is a filled block
//! on its tile, the exit tile is a coloured door (locked / open), and each
//! waiting number is its `Bitmap8x8` digit glyph, integer-scaled to the
//! largest size that fits and centred in its tile — chunky 8-bit digits that
//! keep their proportion as tiles grow and need no font, so this layer stays
//! fully deterministic.
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
    /// Integer magnification for the digit glyphs: the largest scale whose
    /// 8px glyph still fits the tile (10px tiles → 1x, 20px tiles → 2x), so
    /// digits keep their proportion as the maze is retuned.
    digit_scale: u32,
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
    /// A scene of `game` at `tile_px`, centred in the band of `virtual_size`
    /// below the config's HUD strip — each level brings its own tile size and
    /// maze shape, so the origin is computed, never authored.
    #[must_use]
    pub fn new(config: &SceneConfig, tile_px: u32, virtual_size: Size, game: &MazeGame) -> Self {
        let maze = game.maze();
        let band_top = config.hud_strip as i32;
        let grid_w_px = maze.width() as i32 * tile_px as i32;
        let grid_h_px = maze.height() as i32 * tile_px as i32;
        let origin = Point::new(
            (virtual_size.w as i32 - grid_w_px) / 2,
            band_top + (virtual_size.h as i32 - band_top - grid_h_px) / 2,
        );
        let mut scene = Self {
            tile_px,
            digit_scale: (tile_px / 8).max(1),
            origin,
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
        let next = game.next_expected();
        self.digits = game
            .collectibles()
            .iter()
            .map(|c| {
                let glyph = char::from_digit(u32::from(c.value), 10)
                    .expect("collectible values are single digits by construction");
                // The digit whose turn it is glows; the rest sit dim (they
                // are solid, like the bars, until their turn).
                let ink = if Some(c.value) == next {
                    self.colors.digit_next
                } else {
                    self.colors.digit
                };
                let sprite = Bitmap8x8.glyph(glyph).to_sprite(ink);
                let scaled = Size::new(
                    sprite.size().w * self.digit_scale,
                    sprite.size().h * self.digit_scale,
                );
                (
                    self.centered_in_tile(Point::new(c.at.x, c.at.y), scaled),
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
            screen.draw_sprite_scaled(sprite, self.digit_scale, *at);
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

    /// A scene config with no HUD band and each element a distinct colour;
    /// tests pass a screen exactly the grid's size so the maze centres at the
    /// origin and pixel coordinates stay literal.
    fn scene_config() -> SceneConfig {
        SceneConfig {
            hud_strip: 0,
            hud_at: Point::ORIGIN,
            colors: SceneColors {
                wall: Color::rgb(0, 0, 255),
                player: Color::rgb(255, 255, 0),
                digit: Color::rgb(120, 120, 120),
                digit_next: Color::rgb(255, 255, 255),
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
        let scene = MazeScene::new(&config, 10, Size::new(50, 30), &game);
        let mut screen = Surface::new(Size::new(50, 30), Color::rgb(0, 0, 0));

        scene.render(&mut screen);

        // A wall tile is solid bar colour, corner to corner (10px bars).
        assert_eq!(pixel(&screen, 0, 0), config.colors.wall.packed());
        assert_eq!(pixel(&screen, 9, 9), config.colors.wall.packed());
        // The block fills its 10px tile.
        assert_eq!(pixel(&screen, 10, 10), config.colors.player.packed());
        assert_eq!(pixel(&screen, 19, 19), config.colors.player.packed());
        // The lone digit is due next, so its ink glows; the door is locked.
        assert!(tile_holds(&screen, (2, 1), config.colors.digit_next));
        assert!(tile_holds(&screen, (3, 1), config.colors.exit_locked));
        assert!(!tile_holds(&screen, (3, 1), config.colors.exit_open));
    }

    #[test]
    fn only_the_due_digit_glows() {
        // A ring with digit 1 down-left and digit 2 up-right: 1 glows, 2 sits
        // dim; collecting 1 hands the glow to 2.
        let maze = Maze::from_rows(&["#####", "#...#", "#.#.#", "#...#", "#####"]).expect("map");
        let mut game = MazeGame::with_maze(
            maze,
            Tile::new(1, 1),
            Tile::new(3, 3),
            vec![(Tile::new(1, 3), 1), (Tile::new(3, 1), 2)],
        )
        .expect("valid game");
        let config = scene_config();
        let mut scene = MazeScene::new(&config, 10, Size::new(50, 50), &game);
        let mut screen = Surface::new(Size::new(50, 50), Color::rgb(0, 0, 0));
        scene.render(&mut screen);
        assert!(tile_holds(&screen, (1, 3), config.colors.digit_next));
        assert!(tile_holds(&screen, (3, 1), config.colors.digit));
        assert!(!tile_holds(&screen, (3, 1), config.colors.digit_next));

        // Fetch digit 1: the glow moves to digit 2.
        game.step(Direction::Down);
        assert_eq!(game.step(Direction::Down).collected, Some(1));
        scene.sync(&game);
        let mut screen = Surface::new(Size::new(50, 50), Color::rgb(0, 0, 0));
        scene.render(&mut screen);
        assert!(tile_holds(&screen, (3, 1), config.colors.digit_next));
    }

    #[test]
    fn the_door_turns_open_after_the_last_number_is_collected() {
        let mut game = corridor_game();
        let config = scene_config();
        let mut scene = MazeScene::new(&config, 10, Size::new(50, 30), &game);

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
    fn digits_scale_up_with_the_tile() {
        // At 20px tiles the 8×8 digit bakes at 2× (16px of ink), keeping its
        // proportion in the doubled maze — taller than a bare 8px glyph could
        // ever paint.
        let game = corridor_game();
        let config = scene_config();
        let scene = MazeScene::new(&config, 20, Size::new(100, 60), &game);
        let mut screen = Surface::new(Size::new(100, 60), Color::rgb(0, 0, 0));
        scene.render(&mut screen);

        // The digit sits in tile (2, 1): pixels x 40..60, y 20..40.
        let mut min_y = u32::MAX;
        let mut max_y = 0;
        for y in 20..40 {
            for x in 40..60 {
                if pixel(&screen, x, y) == config.colors.digit_next.packed() {
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
        }
        assert!(min_y <= max_y, "the digit's ink must show in its tile");
        let ink_height = max_y - min_y + 1;
        assert!(
            ink_height > 8,
            "a doubled tile doubles the digit; got {ink_height}px of ink"
        );
    }

    #[test]
    fn a_frame_composites_through_the_presentation() {
        // The scene is a PixelLayer end to end: composite one frame through
        // Presentation into a window-sized framebuffer and find the upscaled
        // bar colour where the top-left wall tile lands.
        let game = corridor_game();
        let config = scene_config();
        let virtual_size = Size::new(50, 30);
        let scene = MazeScene::new(&config, 10, virtual_size, &game);
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

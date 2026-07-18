//! [`MazeScene`] — the maze as a [`PixelLayer`], drawn in virtual pixels.
//!
//! The static maze — the bars and the exit door — is a `ratgames::tiles`
//! [`TileMap`] over a tiny solid-colour [`TileSet`] (three 1×1 tiles: wall,
//! locked door, open door), laid out by a [`TileGrid`] centred in the band
//! below the HUD strip and painted by a [`TileMapView`]. A `tile_px`-square
//! tile is that 1×1 source tile integer-scaled by `tile_px`, so a 20px tile
//! makes 20px-wide bars. The player block and the waiting numbers are *actors*
//! drawn over the tile view: the block is a filled tile, each number its
//! `Bitmap8x8` digit glyph, integer-scaled to the largest size that fits and
//! centred in its tile — chunky 8-bit digits that keep their proportion as
//! tiles grow and need no font, so this layer stays fully deterministic.
//!
//! The scene is a *snapshot* of the game, not a live borrow: layers are
//! collected by reference each frame, so the screen re-[`sync`](MazeScene::sync)s
//! the snapshot after every step (cheap at maze scale) rather than holding a
//! self-referential view.

use mazegame_core::MazeGame;
use ratgames::{
    Bitmap8x8, GlyphSource, PixelLayer, Point, Rect, Size, Sprite, Surface, TileGrid, TileId,
    TileLayer, TileMap, TileMapView, TileSet, TileSetId, TileSets,
};

use crate::config::{SceneColors, SceneConfig};

/// The scene's one tileset, and its three solid-colour tiles.
const TILESET: TileSetId = TileSetId::new(0);
const WALL: TileId = TileId::new(0);
const DOOR_LOCKED: TileId = TileId::new(1);
const DOOR_OPEN: TileId = TileId::new(2);

/// A drawable snapshot of one [`MazeGame`] frame. Build with
/// [`new`](Self::new), refresh with [`sync`](Self::sync), and push into the
/// world layers each frame.
pub struct MazeScene {
    /// Integer magnification for the digit glyphs: the largest scale whose
    /// 8px glyph still fits the tile (10px tiles → 1x, 20px tiles → 2x), so
    /// digits keep their proportion as the maze is retuned.
    digit_scale: u32,
    colors: SceneColors,
    /// Tile↔pixel layout: origin, `tile_px` square tiles, maze extent.
    grid: TileGrid,
    /// The solid-colour wall/door tileset the map resolves against.
    tilesets: TileSets,
    /// The static maze: wall cells, floor as empty, the exit as a door tile.
    map: TileMap,
    /// The player's tile.
    player: Point,
    /// Each waiting number: its baked digit sprite at its virtual position.
    digits: Vec<(Point, Sprite)>,
}

impl MazeScene {
    /// A scene of `game` at `tile_px`, centred in the band of `virtual_size`
    /// below the config's HUD strip — each level brings its own tile size and
    /// maze shape, so the grid is centred, never authored.
    #[must_use]
    pub fn new(config: &SceneConfig, tile_px: u32, virtual_size: Size, game: &MazeGame) -> Self {
        let maze = game.maze();
        let grid_size = Size::new(maze.width() as u32, maze.height() as u32);
        // The band below the HUD strip; the generic grid centres in it.
        let band = Rect::new(
            Point::new(0, config.hud_strip as i32),
            Size::new(
                virtual_size.w,
                virtual_size.h.saturating_sub(config.hud_strip),
            ),
        );
        let grid = TileGrid::centered_in(band, Size::new(tile_px, tile_px), grid_size);
        let mut scene = Self {
            digit_scale: (tile_px / 8).max(1),
            colors: config.colors,
            grid,
            tilesets: TileSets::new(vec![tile_sheet(&config.colors)]),
            map: blank_map(grid_size),
            player: Point::ORIGIN,
            digits: Vec::new(),
        };
        scene.sync(game);
        scene
    }

    /// Re-snapshot `game` — the wall/door tile map, the player's tile, and the
    /// remaining digits. Full rebuild every time: at maze scale that is a few
    /// hundred cells and a handful of 8×8 glyph bakes, and it cannot drift.
    pub fn sync(&mut self, game: &MazeGame) {
        let maze = game.maze();
        let (cols, rows) = (maze.width(), maze.height());
        // Bars are wall tiles; floor is empty; the exit floor tile carries the
        // door (locked until every number is gathered, then open).
        let mut cells: Vec<Option<TileId>> = (0..rows as i32)
            .flat_map(|y| (0..cols as i32).map(move |x| (x, y)))
            .map(|(x, y)| maze.is_wall(mazegame_core::Tile::new(x, y)).then_some(WALL))
            .collect();
        let exit = game.exit();
        let door = if game.exit_open() {
            DOOR_OPEN
        } else {
            DOOR_LOCKED
        };
        cells[exit.y as usize * cols + exit.x as usize] = Some(door);
        self.map = TileMap::new(
            Size::new(cols as u32, rows as u32),
            vec![TileLayer::new(TILESET, cells)],
        )
        .expect("the layer holds exactly one cell per maze tile");

        self.player = Point::new(game.player().x, game.player().y);

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
                    self.grid
                        .centered_in_tile(c.at.x as u32, c.at.y as u32, scaled),
                    sprite,
                )
            })
            .collect();
    }
}

/// The scene's tileset: a 3×1 sheet of 1×1 solid-colour tiles — wall, locked
/// door, open door — integer-scaled to the tile size at render time.
fn tile_sheet(colors: &SceneColors) -> TileSet {
    let mut sheet = Sprite::new(Size::new(3, 1));
    sheet.set(Point::new(0, 0), colors.wall);
    sheet.set(Point::new(1, 0), colors.exit_locked);
    sheet.set(Point::new(2, 0), colors.exit_open);
    TileSet::grid(sheet, Size::new(1, 1), 3)
}

/// An all-empty map of the grid's size — a valid placeholder [`MazeScene::new`]
/// overwrites with the first [`sync`](MazeScene::sync).
fn blank_map(grid_size: Size) -> TileMap {
    TileMap::new(
        grid_size,
        vec![TileLayer::new(TILESET, vec![None; grid_size.area()])],
    )
    .expect("a full grid of empty cells is a valid map")
}

impl PixelLayer for MazeScene {
    fn render(&self, screen: &mut Surface) {
        TileMapView::new(&self.map, &self.grid, &self.tilesets).render(screen);
        for (at, sprite) in &self.digits {
            screen.draw_sprite_scaled(sprite, self.digit_scale, *at);
        }
        // The block draws last, so it reads on top of the door when standing
        // on it.
        screen.fill_rect(
            self.grid
                .tile_rect(self.player.x as u32, self.player.y as u32),
            self.colors.player,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazegame_core::{Direction, Maze, Tile};
    use ratgames::{Color, Presentation};

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

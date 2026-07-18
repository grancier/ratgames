//! The maze itself: a rectangular **tile grid** where every tile is wall or
//! floor.
//!
//! A generated maze follows the classic odd-grid layout: a maze of `w × h`
//! *cells* becomes a grid of `(2w+1) × (2h+1)` *tiles* — odd/odd tiles are the
//! cells (always floor), the tiles between two cells are the walls the
//! generator carves through, and the border is solid. Generation is a seeded
//! iterative depth-first backtracker, so it produces a **perfect maze**: every
//! cell reachable, exactly one path between any two cells, long winding
//! corridors.
//!
//! Mazes can also be authored directly ([`Maze::from_rows`]) for tests and
//! hand-built maps; a maze is just the grid, it holds no game state.

use crate::rng::Rng;

/// A tile coordinate on the maze grid. Signed so a consumer can probe one step
/// past any edge and simply read "wall" back ([`Maze::is_wall`] treats out of
/// bounds as solid).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    pub x: i32,
    pub y: i32,
}

impl Tile {
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Why a [`Maze`] could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MazeError {
    /// A generated maze needs at least one cell on each axis.
    ZeroCells,
    /// An authored maze needs at least one row and one column.
    Empty,
    /// An authored maze's rows must all be the same width.
    RaggedRows { row: usize },
    /// An authored row held a glyph other than `'#'` (wall) or `'.'` (floor).
    UnknownGlyph { row: usize, glyph: char },
}

impl std::fmt::Display for MazeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCells => write!(f, "a maze needs at least 1x1 cells"),
            Self::Empty => write!(f, "an authored maze needs at least one row and one column"),
            Self::RaggedRows { row } => {
                write!(f, "row {row} has a different width from row 0")
            }
            Self::UnknownGlyph { row, glyph } => {
                write!(
                    f,
                    "row {row} holds {glyph:?}; use '#' (wall) or '.' (floor)"
                )
            }
        }
    }
}

impl std::error::Error for MazeError {}

/// A rectangular tile grid: each tile is wall or floor. Build one with
/// [`generate`](Self::generate) (seeded perfect maze) or
/// [`from_rows`](Self::from_rows) (authored map); read it with
/// [`is_wall`](Self::is_wall) / [`is_open`](Self::is_open).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Maze {
    width: usize,
    height: usize,
    /// Row-major floor flags: `true` = floor, `false` = wall.
    open: Vec<bool>,
}

impl Maze {
    /// Generate a perfect maze of `cells_w × cells_h` cells — a
    /// `(2·cells_w+1) × (2·cells_h+1)` tile grid with a solid border — by
    /// iterative depth-first backtracking over `rng`. Deterministic: equal
    /// seeds carve equal mazes.
    ///
    /// # Errors
    /// [`MazeError::ZeroCells`] if either axis has no cells.
    pub fn generate(cells_w: usize, cells_h: usize, rng: &mut Rng) -> Result<Self, MazeError> {
        if cells_w == 0 || cells_h == 0 {
            return Err(MazeError::ZeroCells);
        }
        let width = 2 * cells_w + 1;
        let height = 2 * cells_h + 1;
        let mut open = vec![false; width * height];
        let cell_tile = |cx: usize, cy: usize| (2 * cy + 1) * width + (2 * cx + 1);

        // Iterative depth-first backtracker: from the current cell pick an
        // unvisited neighbour at random, knock through the wall tile between
        // them, and descend; retreat when boxed in. Visiting every cell
        // exactly once makes the maze perfect (one path between any two
        // cells) and leaves the border untouched (solid).
        let mut visited = vec![false; cells_w * cells_h];
        let mut stack = vec![(0usize, 0usize)];
        visited[0] = true;
        open[cell_tile(0, 0)] = true;
        while let Some(&(cx, cy)) = stack.last() {
            let mut neighbours: Vec<(usize, usize)> = Vec::with_capacity(4);
            for (dx, dy) in [(0i32, -1i32), (0, 1), (-1, 0), (1, 0)] {
                let nx = cx as i32 + dx;
                let ny = cy as i32 + dy;
                if (0..cells_w as i32).contains(&nx)
                    && (0..cells_h as i32).contains(&ny)
                    && !visited[ny as usize * cells_w + nx as usize]
                {
                    neighbours.push((nx as usize, ny as usize));
                }
            }
            match neighbours.is_empty() {
                true => {
                    stack.pop();
                }
                false => {
                    let (nx, ny) = neighbours[rng.index(neighbours.len())];
                    // The wall tile between two adjacent cells sits halfway
                    // between their cell tiles.
                    open[(cy + ny + 1) * width + (cx + nx + 1)] = true;
                    open[cell_tile(nx, ny)] = true;
                    visited[ny * cells_w + nx] = true;
                    stack.push((nx, ny));
                }
            }
        }
        Ok(Self {
            width,
            height,
            open,
        })
    }

    /// An authored maze from rows of `'#'` (wall) and `'.'` (floor), top to
    /// bottom. Rows must be non-empty and equal width; the map is taken as
    /// written (no border is added or required — out-of-bounds reads are walls
    /// regardless).
    ///
    /// # Errors
    /// [`MazeError`] naming the first malformed row.
    pub fn from_rows(rows: &[&str]) -> Result<Self, MazeError> {
        let height = rows.len();
        let width = rows.first().map_or(0, |row| row.chars().count());
        if height == 0 || width == 0 {
            return Err(MazeError::Empty);
        }
        let mut open = Vec::with_capacity(width * height);
        for (row, text) in rows.iter().enumerate() {
            if text.chars().count() != width {
                return Err(MazeError::RaggedRows { row });
            }
            for glyph in text.chars() {
                open.push(match glyph {
                    '#' => false,
                    '.' => true,
                    other => return Err(MazeError::UnknownGlyph { row, glyph: other }),
                });
            }
        }
        Ok(Self {
            width,
            height,
            open,
        })
    }

    /// Grid width in tiles.
    #[must_use]
    pub fn width(&self) -> usize {
        self.width
    }

    /// Grid height in tiles.
    #[must_use]
    pub fn height(&self) -> usize {
        self.height
    }

    /// Whether `tile` is floor. Out of bounds is not floor.
    #[must_use]
    pub fn is_open(&self, tile: Tile) -> bool {
        if tile.x < 0 || tile.y < 0 {
            return false;
        }
        let (x, y) = (tile.x as usize, tile.y as usize);
        x < self.width && y < self.height && self.open[y * self.width + x]
    }

    /// Whether `tile` is solid to the block — a wall tile, or anywhere outside
    /// the grid.
    #[must_use]
    pub fn is_wall(&self, tile: Tile) -> bool {
        !self.is_open(tile)
    }

    /// Every floor tile, row-major — the deterministic basis for placement.
    #[must_use]
    pub fn open_tiles(&self) -> Vec<Tile> {
        (0..self.height as i32)
            .flat_map(|y| (0..self.width as i32).map(move |x| Tile::new(x, y)))
            .filter(|&tile| self.is_open(tile))
            .collect()
    }

    /// The canonical entry tile of a generated maze: the top-left cell (1, 1).
    /// (For an authored maze this is just that corner tile — the game
    /// validates that whatever start it is given is floor.)
    #[must_use]
    pub fn start(&self) -> Tile {
        Tile::new(1, 1)
    }

    /// The canonical end tile of a generated maze: the bottom-right cell
    /// `(width−2, height−2)`.
    #[must_use]
    pub fn exit(&self) -> Tile {
        Tile::new(self.width as i32 - 2, self.height as i32 - 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn generated(cells_w: usize, cells_h: usize, seed: u64) -> Maze {
        Maze::generate(cells_w, cells_h, &mut Rng::new(seed)).expect("valid cell counts")
    }

    /// Every open tile reachable from `from` by 4-way steps.
    fn flood(maze: &Maze, from: Tile) -> Vec<Tile> {
        let mut seen = vec![false; maze.width() * maze.height()];
        let mut stack = vec![from];
        let mut reached = Vec::new();
        while let Some(tile) = stack.pop() {
            if maze.is_wall(tile) {
                continue;
            }
            let index = tile.y as usize * maze.width() + tile.x as usize;
            if seen[index] {
                continue;
            }
            seen[index] = true;
            reached.push(tile);
            for (dx, dy) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
                stack.push(Tile::new(tile.x + dx, tile.y + dy));
            }
        }
        reached
    }

    #[test]
    fn generate_sizes_the_tile_grid_from_the_cells() {
        let maze = generated(15, 7, 1);
        assert_eq!(maze.width(), 31);
        assert_eq!(maze.height(), 15);
    }

    #[test]
    fn generate_keeps_the_border_solid() {
        let maze = generated(6, 4, 2);
        for x in 0..maze.width() as i32 {
            assert!(maze.is_wall(Tile::new(x, 0)), "top border at x={x}");
            assert!(
                maze.is_wall(Tile::new(x, maze.height() as i32 - 1)),
                "bottom border at x={x}"
            );
        }
        for y in 0..maze.height() as i32 {
            assert!(maze.is_wall(Tile::new(0, y)), "left border at y={y}");
            assert!(
                maze.is_wall(Tile::new(maze.width() as i32 - 1, y)),
                "right border at y={y}"
            );
        }
    }

    #[test]
    fn generate_opens_every_cell_and_connects_the_whole_floor() {
        // A perfect maze: every odd/odd cell tile is floor, and every floor
        // tile — cells and carved walls alike — is reachable from the start,
        // so the exit always is too.
        let maze = generated(8, 5, 42);
        for cy in 0..5 {
            for cx in 0..8 {
                let cell = Tile::new(2 * cx + 1, 2 * cy + 1);
                assert!(maze.is_open(cell), "cell tile {cell:?} must be floor");
            }
        }
        let reached = flood(&maze, maze.start());
        assert_eq!(
            reached.len(),
            maze.open_tiles().len(),
            "every floor tile is reachable from the start"
        );
        assert!(reached.contains(&maze.exit()), "the exit is reachable");
    }

    #[test]
    fn generate_is_deterministic_per_seed() {
        assert_eq!(generated(8, 5, 7), generated(8, 5, 7));
        assert_ne!(
            generated(8, 5, 7),
            generated(8, 5, 8),
            "different seeds carve different mazes"
        );
    }

    #[test]
    fn generate_rejects_zero_cells() {
        assert_eq!(
            Maze::generate(0, 4, &mut Rng::new(1)),
            Err(MazeError::ZeroCells)
        );
        assert_eq!(
            Maze::generate(4, 0, &mut Rng::new(1)),
            Err(MazeError::ZeroCells)
        );
    }

    #[test]
    fn from_rows_reads_walls_and_floor_as_written() {
        let maze = Maze::from_rows(&["#####", "#...#", "#####"]).expect("well-formed map");
        assert_eq!(maze.width(), 5);
        assert_eq!(maze.height(), 3);
        assert!(maze.is_wall(Tile::new(0, 0)));
        assert!(maze.is_open(Tile::new(1, 1)));
        assert!(maze.is_open(Tile::new(3, 1)));
        assert!(maze.is_wall(Tile::new(4, 1)));
        assert_eq!(
            maze.open_tiles(),
            vec![Tile::new(1, 1), Tile::new(2, 1), Tile::new(3, 1)]
        );
    }

    #[test]
    fn from_rows_rejects_malformed_maps() {
        assert_eq!(Maze::from_rows(&[]), Err(MazeError::Empty));
        assert_eq!(Maze::from_rows(&["", ""]), Err(MazeError::Empty));
        assert_eq!(
            Maze::from_rows(&["###", "##"]),
            Err(MazeError::RaggedRows { row: 1 })
        );
        assert_eq!(
            Maze::from_rows(&["###", "#x#"]),
            Err(MazeError::UnknownGlyph { row: 1, glyph: 'x' })
        );
    }

    #[test]
    fn out_of_bounds_is_solid_wall() {
        let maze = Maze::from_rows(&["..", ".."]).expect("open map");
        assert!(maze.is_wall(Tile::new(-1, 0)));
        assert!(maze.is_wall(Tile::new(0, -1)));
        assert!(maze.is_wall(Tile::new(2, 0)));
        assert!(maze.is_wall(Tile::new(0, 2)));
        assert!(maze.is_open(Tile::new(1, 1)));
    }
}

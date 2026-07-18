//! One run of the maze game: the player's block on a [`Maze`], the numbers it
//! must collect, and the exit that wins the run.
//!
//! The block occupies exactly one floor tile. One [`step`](MazeGame::step) in
//! a [`Direction`] moves it one tile — or not at all when the target tile is
//! wall (the bars are solid). Entering a number's tile collects it; once every
//! number is collected the exit **opens**, and stepping onto the open exit
//! wins the run. A finished run is inert: further steps move nothing (the same
//! past-the-end contract as `ratgames::GameRun::record_attempt`).

use crate::maze::{Maze, MazeError, Tile};
use crate::rng::Rng;

/// The four ways the block can step. One step is one tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    /// The tile delta of one step.
    const fn delta(self) -> (i32, i32) {
        match self {
            Self::Up => (0, -1),
            Self::Down => (0, 1),
            Self::Left => (-1, 0),
            Self::Right => (1, 0),
        }
    }
}

/// Where the run stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The block is moving; numbers remain or the exit is still ahead.
    Playing,
    /// The block reached the open exit with every number collected.
    Won,
}

/// One number waiting on the floor: where it sits and the digit it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Collectible {
    pub at: Tile,
    /// A single digit, `1..=9`.
    pub value: u8,
}

/// What one [`step`](MazeGame::step) did: whether the block moved, the number
/// it collected (if it entered one), and whether it won the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepOutcome {
    /// The block moved one tile (`false`: a wall blocked it, or the run was
    /// already over).
    pub moved: bool,
    /// The digit collected by entering its tile this step.
    pub collected: Option<u8>,
    /// This step won the run (entered the open exit).
    pub won: bool,
}

/// Why a [`MazeGame`] could not be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MazeGameError {
    /// The maze itself was malformed.
    Maze(MazeError),
    /// A tile that must be floor (start, exit, or a number's tile) is wall.
    ClosedTile { which: &'static str, at: Tile },
    /// The start and exit are the same tile — the run would be won at birth.
    StartIsExit { at: Tile },
    /// Two numbers share a tile, or one sits on the start or exit.
    OccupiedTile { at: Tile },
    /// A number's value is not a single digit `1..=9`.
    ValueOutOfRange { value: usize },
    /// More numbers were requested than the maze has free floor tiles.
    TooManyCollectibles { requested: usize, available: usize },
}

impl std::fmt::Display for MazeGameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Maze(e) => write!(f, "invalid maze: {e}"),
            Self::ClosedTile { which, at } => {
                write!(f, "the {which} tile ({}, {}) is not floor", at.x, at.y)
            }
            Self::StartIsExit { at } => {
                write!(f, "start and exit are both ({}, {})", at.x, at.y)
            }
            Self::OccupiedTile { at } => {
                write!(f, "tile ({}, {}) is already taken", at.x, at.y)
            }
            Self::ValueOutOfRange { value } => {
                write!(f, "a number must be a single digit 1..=9, got {value}")
            }
            Self::TooManyCollectibles {
                requested,
                available,
            } => {
                write!(
                    f,
                    "{requested} numbers requested but only {available} free floor tiles"
                )
            }
        }
    }
}

impl std::error::Error for MazeGameError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Maze(e) => Some(e),
            _ => None,
        }
    }
}

impl From<MazeError> for MazeGameError {
    fn from(error: MazeError) -> Self {
        Self::Maze(error)
    }
}

/// One run: the maze, the block, the numbers still on the floor, and the
/// phase. Build a random run with [`new`](Self::new) or a hand-laid one with
/// [`with_maze`](Self::with_maze); drive it with [`step`](Self::step).
#[derive(Debug, Clone)]
pub struct MazeGame {
    maze: Maze,
    player: Tile,
    exit: Tile,
    /// The numbers still on the floor; collecting removes them.
    collectibles: Vec<Collectible>,
    total: usize,
    phase: Phase,
}

impl MazeGame {
    /// A random run: generate a `cells_w × cells_h` perfect maze from `seed`,
    /// start the block at the top-left cell, put the exit at the bottom-right
    /// cell, and scatter `collectible_count` numbers (valued `1..=count`) over
    /// distinct free floor tiles — never the start or exit, so the last number
    /// can never win the run by itself.
    ///
    /// # Errors
    /// [`MazeGameError`] if the maze is degenerate, the count exceeds `9` (a
    /// number shows one digit) or the free floor.
    pub fn new(
        cells_w: usize,
        cells_h: usize,
        collectible_count: usize,
        seed: u64,
    ) -> Result<Self, MazeGameError> {
        if collectible_count > 9 {
            return Err(MazeGameError::ValueOutOfRange {
                value: collectible_count,
            });
        }
        let mut rng = Rng::new(seed);
        let maze = Maze::generate(cells_w, cells_h, &mut rng)?;
        let (start, exit) = (maze.start(), maze.exit());
        if start == exit {
            return Err(MazeGameError::StartIsExit { at: start });
        }
        // The free floor in row-major order; a partial Fisher–Yates over it
        // draws `collectible_count` distinct tiles reproducibly per seed.
        let mut free: Vec<Tile> = maze
            .open_tiles()
            .into_iter()
            .filter(|&tile| tile != start && tile != exit)
            .collect();
        if collectible_count > free.len() {
            return Err(MazeGameError::TooManyCollectibles {
                requested: collectible_count,
                available: free.len(),
            });
        }
        let mut numbers = Vec::with_capacity(collectible_count);
        for index in 0..collectible_count {
            let swap_with = index + rng.index(free.len() - index);
            free.swap(index, swap_with);
            numbers.push((free[index], (index + 1) as u8));
        }
        Self::with_maze(maze, start, exit, numbers)
    }

    /// A run over an authored maze: the block starts at `start`, the exit is
    /// `exit`, and `numbers` are placed as given.
    ///
    /// # Errors
    /// [`MazeGameError`] if the start, exit, or any number tile is wall, the
    /// start is the exit, a tile is used twice, or a value is not `1..=9`.
    pub fn with_maze(
        maze: Maze,
        start: Tile,
        exit: Tile,
        numbers: Vec<(Tile, u8)>,
    ) -> Result<Self, MazeGameError> {
        if maze.is_wall(start) {
            return Err(MazeGameError::ClosedTile {
                which: "start",
                at: start,
            });
        }
        if maze.is_wall(exit) {
            return Err(MazeGameError::ClosedTile {
                which: "exit",
                at: exit,
            });
        }
        if start == exit {
            return Err(MazeGameError::StartIsExit { at: start });
        }
        let mut collectibles: Vec<Collectible> = Vec::with_capacity(numbers.len());
        for (at, value) in numbers {
            if !(1..=9).contains(&value) {
                return Err(MazeGameError::ValueOutOfRange {
                    value: value as usize,
                });
            }
            if maze.is_wall(at) {
                return Err(MazeGameError::ClosedTile {
                    which: "number",
                    at,
                });
            }
            if at == start || at == exit || collectibles.iter().any(|c| c.at == at) {
                return Err(MazeGameError::OccupiedTile { at });
            }
            collectibles.push(Collectible { at, value });
        }
        let total = collectibles.len();
        Ok(Self {
            maze,
            player: start,
            exit,
            collectibles,
            total,
            phase: Phase::Playing,
        })
    }

    /// Step the block one tile in `direction`. A wall blocks the step (the
    /// block stays put); entering a number's tile collects it; entering the
    /// exit once every number is collected wins the run. Inert once the run is
    /// won.
    pub fn step(&mut self, direction: Direction) -> StepOutcome {
        const HELD: StepOutcome = StepOutcome {
            moved: false,
            collected: None,
            won: false,
        };
        if self.phase != Phase::Playing {
            return HELD;
        }
        let (dx, dy) = direction.delta();
        let target = Tile::new(self.player.x + dx, self.player.y + dy);
        if self.maze.is_wall(target) {
            return HELD;
        }
        self.player = target;
        let collected = self
            .collectibles
            .iter()
            .position(|c| c.at == target)
            .map(|index| self.collectibles.remove(index).value);
        let won = self.exit_open() && target == self.exit;
        if won {
            self.phase = Phase::Won;
        }
        StepOutcome {
            moved: true,
            collected,
            won,
        }
    }

    #[must_use]
    pub fn maze(&self) -> &Maze {
        &self.maze
    }

    /// The tile the block occupies.
    #[must_use]
    pub fn player(&self) -> Tile {
        self.player
    }

    /// The exit tile.
    #[must_use]
    pub fn exit(&self) -> Tile {
        self.exit
    }

    #[must_use]
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The numbers still waiting on the floor.
    #[must_use]
    pub fn collectibles(&self) -> &[Collectible] {
        &self.collectibles
    }

    /// Numbers collected so far.
    #[must_use]
    pub fn collected(&self) -> usize {
        self.total - self.collectibles.len()
    }

    /// Numbers the run started with.
    #[must_use]
    pub fn total(&self) -> usize {
        self.total
    }

    /// Whether the exit is open — every number collected. (A run with no
    /// numbers starts open.)
    #[must_use]
    pub fn exit_open(&self) -> bool {
        self.collectibles.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-corridor maze: floor at (1,1)..(3,1).
    fn corridor() -> Maze {
        Maze::from_rows(&["#####", "#...#", "#####"]).expect("well-formed map")
    }

    /// A ring around a centre pillar — two routes between any two floor tiles.
    fn ring() -> Maze {
        Maze::from_rows(&["#####", "#...#", "#.#.#", "#...#", "#####"]).expect("well-formed map")
    }

    fn game(maze: Maze, exit: Tile, numbers: Vec<(Tile, u8)>) -> MazeGame {
        MazeGame::with_maze(maze, Tile::new(1, 1), exit, numbers).expect("valid game")
    }

    #[test]
    fn walls_block_and_floor_permits_one_tile_steps() {
        let mut game = game(corridor(), Tile::new(3, 1), vec![]);

        let blocked = game.step(Direction::Up);
        assert!(!blocked.moved, "the bar above is solid");
        assert_eq!(game.player(), Tile::new(1, 1));

        let step = game.step(Direction::Right);
        assert!(step.moved);
        assert_eq!(step.collected, None);
        assert_eq!(game.player(), Tile::new(2, 1), "one step is one tile");
    }

    #[test]
    fn entering_a_numbers_tile_collects_it() {
        let mut game = game(corridor(), Tile::new(3, 1), vec![(Tile::new(2, 1), 7)]);
        assert_eq!(game.total(), 1);
        assert!(!game.exit_open());

        let step = game.step(Direction::Right);

        assert_eq!(step.collected, Some(7));
        assert_eq!(game.collected(), 1);
        assert!(game.collectibles().is_empty());
        assert!(
            game.exit_open(),
            "collecting the last number opens the exit"
        );
    }

    #[test]
    fn the_exit_stays_shut_until_every_number_is_collected() {
        // The ring offers a route to the exit that avoids the number: reaching
        // the exit early must not win.
        let mut game = game(ring(), Tile::new(3, 3), vec![(Tile::new(3, 1), 5)]);
        game.step(Direction::Down);
        game.step(Direction::Down);
        let early = game.step(Direction::Right);
        let arrive = game.step(Direction::Right);
        assert_eq!(game.player(), Tile::new(3, 3), "standing on the shut exit");
        assert!(!early.won && !arrive.won);
        assert_eq!(game.phase(), Phase::Playing);

        // Fetch the number, come back: now the same tile wins.
        game.step(Direction::Up);
        let fetch = game.step(Direction::Up);
        assert_eq!(fetch.collected, Some(5));
        game.step(Direction::Down);
        let winning = game.step(Direction::Down);
        assert!(winning.won);
        assert_eq!(game.phase(), Phase::Won);
    }

    #[test]
    fn a_won_run_is_inert() {
        let mut game = game(corridor(), Tile::new(2, 1), vec![]);
        assert!(game.step(Direction::Right).won);

        let after = game.step(Direction::Right);
        assert!(!after.moved && !after.won);
        assert_eq!(after.collected, None);
        assert_eq!(
            game.player(),
            Tile::new(2, 1),
            "the block stays on the exit"
        );
    }

    #[test]
    fn with_maze_rejects_degenerate_layouts() {
        let wall = Tile::new(0, 0);
        let floor = Tile::new(1, 1);
        assert!(matches!(
            MazeGame::with_maze(corridor(), wall, Tile::new(3, 1), vec![]),
            Err(MazeGameError::ClosedTile { which: "start", .. })
        ));
        assert!(matches!(
            MazeGame::with_maze(corridor(), floor, wall, vec![]),
            Err(MazeGameError::ClosedTile { which: "exit", .. })
        ));
        assert!(matches!(
            MazeGame::with_maze(corridor(), floor, floor, vec![]),
            Err(MazeGameError::StartIsExit { .. })
        ));
        assert!(matches!(
            MazeGame::with_maze(corridor(), floor, Tile::new(3, 1), vec![(wall, 1)]),
            Err(MazeGameError::ClosedTile {
                which: "number",
                ..
            })
        ));
        assert!(matches!(
            MazeGame::with_maze(
                corridor(),
                floor,
                Tile::new(3, 1),
                vec![(Tile::new(2, 1), 1), (Tile::new(2, 1), 2)]
            ),
            Err(MazeGameError::OccupiedTile { .. })
        ));
        assert!(
            matches!(
                MazeGame::with_maze(corridor(), floor, Tile::new(3, 1), vec![(floor, 1)]),
                Err(MazeGameError::OccupiedTile { .. }),
            ),
            "a number on the start tile would collect itself"
        );
        assert!(matches!(
            MazeGame::with_maze(
                corridor(),
                floor,
                Tile::new(3, 1),
                vec![(Tile::new(2, 1), 0)]
            ),
            Err(MazeGameError::ValueOutOfRange { value: 0 })
        ));
        assert!(matches!(
            MazeGame::with_maze(
                corridor(),
                floor,
                Tile::new(3, 1),
                vec![(Tile::new(2, 1), 10)]
            ),
            Err(MazeGameError::ValueOutOfRange { value: 10 })
        ));
    }

    #[test]
    fn new_scatters_the_requested_numbers_over_free_floor() {
        let game = MazeGame::new(8, 5, 5, 42).expect("valid run");
        assert_eq!(game.total(), 5);
        assert_eq!(game.collected(), 0);
        assert!(!game.exit_open());

        let mut values: Vec<u8> = game.collectibles().iter().map(|c| c.value).collect();
        values.sort_unstable();
        assert_eq!(values, vec![1, 2, 3, 4, 5], "digits are 1..=count");

        for (index, c) in game.collectibles().iter().enumerate() {
            assert!(game.maze().is_open(c.at), "numbers sit on floor");
            assert_ne!(c.at, game.player(), "never on the start");
            assert_ne!(c.at, game.exit(), "never on the exit");
            for other in &game.collectibles()[index + 1..] {
                assert_ne!(c.at, other.at, "numbers sit on distinct tiles");
            }
        }
    }

    #[test]
    fn new_is_deterministic_per_seed() {
        let a = MazeGame::new(8, 5, 4, 7).expect("valid run");
        let b = MazeGame::new(8, 5, 4, 7).expect("valid run");
        assert_eq!(a.maze(), b.maze());
        assert_eq!(a.collectibles(), b.collectibles());
    }

    #[test]
    fn new_rejects_impossible_requests() {
        // 1×1 cells: the only floor tile is both start and exit.
        assert!(matches!(
            MazeGame::new(1, 1, 0, 1),
            Err(MazeGameError::StartIsExit { .. })
        ));
        // 2×1 cells: three floor tiles minus start and exit leaves one free.
        assert!(matches!(
            MazeGame::new(2, 1, 2, 1),
            Err(MazeGameError::TooManyCollectibles {
                requested: 2,
                available: 1
            })
        ));
        // A number shows a single digit, so at most nine.
        assert!(matches!(
            MazeGame::new(8, 5, 10, 1),
            Err(MazeGameError::ValueOutOfRange { value: 10 })
        ));
    }
}

//! One run of the maze game: the player's block on a [`Maze`], the numbers it
//! must collect, and the exit that wins the run.
//!
//! The block occupies exactly one floor tile. One [`step`](MazeGame::step) in
//! a [`Direction`] moves it one tile — or not at all when the target tile is
//! wall (the bars are solid). Numbers are gathered **in order**: only the
//! lowest digit still on the floor can be collected (by entering its tile),
//! and every other digit is *solid* — it blocks the corridor exactly like a
//! bar until its turn comes, so the player may have to bump into a blocker,
//! backtrack for the digit that is due, and return. Once every number is
//! collected the exit **opens**, and stepping onto the open exit wins the
//! run. A finished run is inert: further steps move nothing (the same
//! past-the-end contract as `ratgames::GameRun::record_attempt`).
//!
//! [`MazeGame::new`] places its digits so this is always winnable: the picked
//! tiles are numbered by their first-visit order in a depth-first walk from
//! the start, so the route to whichever digit is due only ever crosses tiles
//! that were visited earlier — i.e. digits already collected. A digit can sit
//! in front of the exit route only when everything earlier is behind it.

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
    /// Two numbers share a value — the gathering order would be ambiguous.
    DuplicateValue { value: u8 },
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
            Self::DuplicateValue { value } => {
                write!(
                    f,
                    "the digit {value} is placed twice; the gathering order needs distinct values"
                )
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

/// The parameters for dealing a random run, and the single owner of the rules a
/// deal must satisfy *before* a maze is generated. A consumer (the app's config
/// validation) checks a candidate rung against these without paying to carve a
/// maze; [`MazeGame::from_spec`] re-checks them on the deal path, so the closed
/// form and the digit bound live here, in the domain, not copied into callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MazeSpec {
    /// Maze width in cells (a `cells_w × cells_h` maze is a
    /// `(2·cells_w+1) × (2·cells_h+1)` tile grid).
    pub cells_w: usize,
    /// Maze height in cells.
    pub cells_h: usize,
    /// How many numbers to scatter, valued `1..=count`, gathered in order.
    pub collectible_count: usize,
    /// Growing-tree branching `0..=100`; out-of-range values are clamped by
    /// [`Maze::generate_with`], so this is *not* a validation error here.
    pub branch_chance: u8,
}

impl MazeSpec {
    /// Whether a run of this shape can be dealt: at least one cell per axis, a
    /// start distinct from the exit (a 1×1 maze has neither), a single-digit
    /// count, and no more numbers than the maze's free floor can hold. A perfect
    /// maze of `c = cells_w·cells_h` cells has `2c−1` floor tiles; minus the
    /// start and exit that leaves `2c−3` for numbers.
    ///
    /// # Errors
    /// [`MazeSpecError`] naming the first rule broken.
    pub fn validate(&self) -> Result<(), MazeSpecError> {
        if self.cells_w == 0 || self.cells_h == 0 {
            return Err(MazeSpecError::ZeroCells);
        }
        if self.collectible_count > 9 {
            return Err(MazeSpecError::ValueOutOfRange {
                value: self.collectible_count,
            });
        }
        if self.cells_w == 1 && self.cells_h == 1 {
            return Err(MazeSpecError::StartIsExit);
        }
        let available = (2 * self.cells_w * self.cells_h).saturating_sub(3);
        if self.collectible_count > available {
            return Err(MazeSpecError::TooManyCollectibles {
                requested: self.collectible_count,
                available,
            });
        }
        Ok(())
    }
}

/// Why a [`MazeSpec`] cannot be dealt. Maps onto the matching [`MazeGameError`]
/// on the deal path (see the `From` impl), so `MazeGame::new`'s error surface is
/// unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MazeSpecError {
    /// A maze needs at least one cell on each axis.
    ZeroCells,
    /// The requested count is not a single digit `1..=9`.
    ValueOutOfRange { value: usize },
    /// A 1×1 maze's start and exit coincide — the run would be won at birth.
    StartIsExit,
    /// More numbers were requested than the maze's free floor can hold.
    TooManyCollectibles { requested: usize, available: usize },
}

impl std::fmt::Display for MazeSpecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroCells => write!(f, "a maze needs at least 1x1 cells"),
            Self::ValueOutOfRange { value } => {
                write!(f, "a number must be a single digit 1..=9, got {value}")
            }
            Self::StartIsExit => write!(f, "a 1x1 maze has start and exit on the same tile"),
            Self::TooManyCollectibles {
                requested,
                available,
            } => write!(
                f,
                "{requested} numbers requested but only {available} free floor tiles"
            ),
        }
    }
}

impl std::error::Error for MazeSpecError {}

impl From<MazeSpecError> for MazeGameError {
    fn from(error: MazeSpecError) -> Self {
        match error {
            MazeSpecError::ZeroCells => Self::Maze(MazeError::ZeroCells),
            MazeSpecError::ValueOutOfRange { value } => Self::ValueOutOfRange { value },
            // A spec-level start/exit clash is only ever the 1×1 maze, whose
            // coincident tile is the canonical start (1, 1).
            MazeSpecError::StartIsExit => Self::StartIsExit {
                at: Tile::new(1, 1),
            },
            MazeSpecError::TooManyCollectibles {
                requested,
                available,
            } => Self::TooManyCollectibles {
                requested,
                available,
            },
        }
    }
}

/// One run: the maze, the block, the numbers still on the floor, and the
/// phase. Build a random run with [`new`](Self::new) or a hand-laid one with
/// [`with_maze`](Self::with_maze); drive it with [`step`](Self::step).
#[derive(Debug, Clone)]
pub struct MazeGame {
    maze: Maze,
    player: Tile,
    start: Tile,
    exit: Tile,
    /// The numbers still on the floor; collecting removes them.
    collectibles: Vec<Collectible>,
    /// The initial placement, kept whole so [`reset`](Self::reset) can restore
    /// the exact same level.
    placed: Vec<Collectible>,
    phase: Phase,
}

impl MazeGame {
    /// A random run: generate a `cells_w × cells_h` perfect maze from `seed`
    /// (branchier at higher `branch_chance` — see [`Maze::generate_with`]),
    /// start the block at the top-left cell, put the exit at the bottom-right
    /// cell, and scatter `collectible_count` numbers over distinct free floor
    /// tiles — never the start or exit.
    ///
    /// The numbers are valued `1..=count` **in the order a depth-first walk
    /// from the start first reaches their tiles**, which makes the sequential
    /// gathering rule always satisfiable: the walk's route to whichever digit
    /// is due crosses only tiles reached earlier — digits already collected —
    /// so a digit blocks an exit route only when everything before it is
    /// behind it, and backtracking always suffices
    /// (`every_generated_run_is_sequentially_solvable` plays this out).
    ///
    /// # Errors
    /// [`MazeGameError`] if the maze is degenerate, the count exceeds `9` (a
    /// number shows one digit) or the free floor.
    pub fn new(
        cells_w: usize,
        cells_h: usize,
        collectible_count: usize,
        branch_chance: u8,
        seed: u64,
    ) -> Result<Self, MazeGameError> {
        Self::from_spec(
            &MazeSpec {
                cells_w,
                cells_h,
                collectible_count,
                branch_chance,
            },
            seed,
        )
    }

    /// A random run for `spec`, seeded with `seed` — the same deal as
    /// [`new`](Self::new), taking the parameters as a validated [`MazeSpec`].
    /// [`MazeSpec::validate`] owns the construction rules (single-digit count,
    /// free-floor headroom, non-degenerate cells), checked up front, so the
    /// generate-and-place body below never has to re-derive them.
    ///
    /// # Errors
    /// [`MazeGameError`] if the spec is invalid (via [`MazeSpecError`]).
    pub fn from_spec(spec: &MazeSpec, seed: u64) -> Result<Self, MazeGameError> {
        spec.validate()?;
        let mut rng = Rng::new(seed);
        let maze = Maze::generate_with(spec.cells_w, spec.cells_h, spec.branch_chance, &mut rng)?;
        let (start, exit) = (maze.start(), maze.exit());
        // The free floor in row-major order; a partial Fisher–Yates over it
        // draws `collectible_count` distinct tiles reproducibly per seed. The
        // spec guarantees the count fits, so no bounds re-check is needed.
        let mut free: Vec<Tile> = maze
            .open_tiles()
            .into_iter()
            .filter(|&tile| tile != start && tile != exit)
            .collect();
        let mut picked = Vec::with_capacity(spec.collectible_count);
        for index in 0..spec.collectible_count {
            let swap_with = index + rng.index(free.len() - index);
            free.swap(index, swap_with);
            picked.push(free[index]);
        }
        // Number the picked tiles in walk order (see the type docs): sorting
        // by first-visit order is what guarantees the sequence is gatherable.
        let order = walk_order(&maze, start);
        picked.sort_by_key(|tile| order[tile.y as usize * maze.width() + tile.x as usize]);
        let numbers = picked
            .into_iter()
            .enumerate()
            .map(|(index, at)| (at, (index + 1) as u8))
            .collect();
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
            if collectibles.iter().any(|c| c.value == value) {
                return Err(MazeGameError::DuplicateValue { value });
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
        Ok(Self {
            maze,
            player: start,
            start,
            exit,
            placed: collectibles.clone(),
            collectibles,
            phase: Phase::Playing,
        })
    }

    /// Restart this exact level: the block back at the start, every number
    /// back on its original tile, the run playing again. The maze is
    /// untouched — deal a different one by building a new game.
    pub fn reset(&mut self) {
        self.player = self.start;
        self.collectibles = self.placed.clone();
        self.phase = Phase::Playing;
    }

    /// Step the block one tile in `direction`. A wall blocks the step (the
    /// block stays put), and so does any digit that is not the one due next —
    /// out-of-turn digits are solid. Entering the due digit's tile collects
    /// it (the tile turns to floor); entering the exit once every number is
    /// collected wins the run. Inert once the run is won.
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
        let mut collected = None;
        if let Some(index) = self.collectibles.iter().position(|c| c.at == target) {
            let value = self.collectibles[index].value;
            if Some(value) != self.next_expected() {
                // An out-of-turn digit blocks like a bar until its turn.
                return HELD;
            }
            self.collectibles.remove(index);
            collected = Some(value);
        }
        self.player = target;
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
        self.placed.len() - self.collectibles.len()
    }

    /// Numbers the run started with.
    #[must_use]
    pub fn total(&self) -> usize {
        self.placed.len()
    }

    /// The digit the run wants next — the smallest still on the floor — or
    /// `None` once every number is gathered. Only this digit can be
    /// collected; all others are solid.
    #[must_use]
    pub fn next_expected(&self) -> Option<u8> {
        self.collectibles.iter().map(|c| c.value).min()
    }

    /// Whether the exit is open — every number collected. (A run with no
    /// numbers starts open.)
    #[must_use]
    pub fn exit_open(&self) -> bool {
        self.collectibles.is_empty()
    }
}

/// First-visit order of every floor tile in a depth-first walk from `from`
/// (fixed neighbour order); `usize::MAX` for walls. The walk's route to any
/// tile runs through tiles it reached earlier, so digits numbered in this
/// order can always be gathered in sequence — nothing later ever seals off a
/// digit that is still due.
fn walk_order(maze: &Maze, from: Tile) -> Vec<usize> {
    let mut order = vec![usize::MAX; maze.width() * maze.height()];
    let mut stack = vec![from];
    let mut next = 0;
    while let Some(tile) = stack.pop() {
        if maze.is_wall(tile) {
            continue;
        }
        let index = tile.y as usize * maze.width() + tile.x as usize;
        if order[index] != usize::MAX {
            continue;
        }
        order[index] = next;
        next += 1;
        for (dx, dy) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
            stack.push(Tile::new(tile.x + dx, tile.y + dy));
        }
    }
    order
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
    fn out_of_order_digits_are_solid_until_their_turn() {
        // Digit 1 hides down the left side of the ring; digit 2 sits on the
        // top route to the exit. The player must bump into 2, backtrack for
        // 1, and return — exactly the intended play pattern.
        let mut game = game(
            ring(),
            Tile::new(3, 3),
            vec![(Tile::new(1, 3), 1), (Tile::new(3, 1), 2)],
        );
        assert_eq!(game.next_expected(), Some(1));

        game.step(Direction::Right); // (2,1)
        let bumped = game.step(Direction::Right); // digit 2's tile
        assert!(!bumped.moved, "digit 2 is solid while 1 is uncollected");
        assert_eq!(bumped.collected, None);
        assert_eq!(game.player(), Tile::new(2, 1));

        // Backtrack around the left side and fetch digit 1.
        game.step(Direction::Left);
        game.step(Direction::Down);
        let one = game.step(Direction::Down);
        assert_eq!(one.collected, Some(1));
        assert_eq!(game.next_expected(), Some(2));

        // Digit 2 now yields; then down to the open exit.
        game.step(Direction::Up);
        game.step(Direction::Up);
        game.step(Direction::Right);
        let two = game.step(Direction::Right);
        assert_eq!(two.collected, Some(2));
        assert_eq!(game.next_expected(), None);
        assert!(game.exit_open());
        game.step(Direction::Down);
        assert!(game.step(Direction::Down).won);
    }

    /// Greedy sequential playthrough as a reachability proof: for each digit
    /// in value order, the path from the previous digit must exist while every
    /// later digit is solid; then the exit must be reachable.
    fn solvable_in_order(game: &MazeGame) -> bool {
        fn reachable(maze: &Maze, from: Tile, to: Tile, solid: &[Tile]) -> bool {
            let mut seen = vec![false; maze.width() * maze.height()];
            let mut stack = vec![from];
            while let Some(tile) = stack.pop() {
                if tile == to {
                    return true;
                }
                if maze.is_wall(tile) || solid.contains(&tile) {
                    continue;
                }
                let index = tile.y as usize * maze.width() + tile.x as usize;
                if seen[index] {
                    continue;
                }
                seen[index] = true;
                for (dx, dy) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
                    stack.push(Tile::new(tile.x + dx, tile.y + dy));
                }
            }
            false
        }
        let mut ordered: Vec<Collectible> = game.collectibles().to_vec();
        ordered.sort_by_key(|c| c.value);
        let mut position = game.player();
        for (index, digit) in ordered.iter().enumerate() {
            let solid: Vec<Tile> = ordered[index + 1..].iter().map(|c| c.at).collect();
            if !reachable(game.maze(), position, digit.at, &solid) {
                return false;
            }
            position = digit.at;
        }
        reachable(game.maze(), position, game.exit(), &[])
    }

    #[test]
    fn every_generated_run_is_sequentially_solvable() {
        // The walk-order placement guarantees a digit can only sit in front
        // of the exit route when everything earlier is behind it — prove it
        // by playing: across seeds, shapes, and branchiness, the greedy
        // in-order run always gets through.
        for seed in 0..12 {
            for &(cells_w, cells_h, digits, branch) in &[
                (7usize, 3usize, 3usize, 0u8),
                (15, 7, 6, 20),
                (31, 15, 9, 60),
            ] {
                let game =
                    MazeGame::new(cells_w, cells_h, digits, branch, seed).expect("a valid run");
                assert!(
                    solvable_in_order(&game),
                    "seed {seed} on {cells_w}x{cells_h}/{digits} digits must be playable"
                );
            }
        }
    }

    #[test]
    fn reset_restores_the_run_to_its_initial_state() {
        let mut game = game(corridor(), Tile::new(3, 1), vec![(Tile::new(2, 1), 7)]);
        assert_eq!(game.step(Direction::Right).collected, Some(7));
        assert!(game.step(Direction::Right).won);

        game.reset();

        assert_eq!(game.player(), Tile::new(1, 1), "back at the start");
        assert_eq!(game.phase(), Phase::Playing);
        assert_eq!(game.collected(), 0);
        assert_eq!(
            game.collectibles(),
            &[Collectible {
                at: Tile::new(2, 1),
                value: 7
            }],
            "the same digits return to the same tiles"
        );
        // The restored run plays: collect and win again.
        assert_eq!(game.step(Direction::Right).collected, Some(7));
        assert!(game.step(Direction::Right).won);
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
        assert!(
            matches!(
                MazeGame::with_maze(
                    ring(),
                    floor,
                    Tile::new(3, 3),
                    vec![(Tile::new(1, 3), 4), (Tile::new(3, 1), 4)]
                ),
                Err(MazeGameError::DuplicateValue { value: 4 })
            ),
            "the gathering order needs every value distinct"
        );
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
        let game = MazeGame::new(8, 5, 5, 0, 42).expect("valid run");
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
        let a = MazeGame::new(8, 5, 4, 25, 7).expect("valid run");
        let b = MazeGame::new(8, 5, 4, 25, 7).expect("valid run");
        assert_eq!(a.maze(), b.maze());
        assert_eq!(a.collectibles(), b.collectibles());
    }

    #[test]
    fn new_rejects_impossible_requests() {
        // 1×1 cells: the only floor tile is both start and exit.
        assert!(matches!(
            MazeGame::new(1, 1, 0, 0, 1),
            Err(MazeGameError::StartIsExit { .. })
        ));
        // 2×1 cells: three floor tiles minus start and exit leaves one free.
        assert!(matches!(
            MazeGame::new(2, 1, 2, 0, 1),
            Err(MazeGameError::TooManyCollectibles {
                requested: 2,
                available: 1
            })
        ));
        // A number shows a single digit, so at most nine.
        assert!(matches!(
            MazeGame::new(8, 5, 10, 0, 1),
            Err(MazeGameError::ValueOutOfRange { value: 10 })
        ));
    }

    fn spec(cells_w: usize, cells_h: usize, count: usize, branch: u8) -> MazeSpec {
        MazeSpec {
            cells_w,
            cells_h,
            collectible_count: count,
            branch_chance: branch,
        }
    }

    #[test]
    fn maze_spec_owns_the_construction_rules() {
        // A comfortable rung validates.
        assert_eq!(spec(8, 5, 5, 0).validate(), Ok(()));
        // Zero cells on either axis.
        assert_eq!(spec(0, 4, 1, 0).validate(), Err(MazeSpecError::ZeroCells));
        // More than a single digit.
        assert_eq!(
            spec(8, 5, 10, 0).validate(),
            Err(MazeSpecError::ValueOutOfRange { value: 10 })
        );
        // A 1×1 maze has start == exit.
        assert_eq!(spec(1, 1, 0, 0).validate(), Err(MazeSpecError::StartIsExit));
        // The closed-form free floor: a 2×1-cell maze frees exactly one tile.
        assert_eq!(
            spec(2, 1, 2, 0).validate(),
            Err(MazeSpecError::TooManyCollectibles {
                requested: 2,
                available: 1,
            })
        );
        assert_eq!(spec(2, 1, 1, 0).validate(), Ok(()), "one number fits");
        // branch_chance is clamped by the generator, not rejected here.
        assert_eq!(spec(8, 5, 5, 200).validate(), Ok(()));
    }

    #[test]
    fn from_spec_deals_the_same_run_as_new() {
        let a = MazeGame::from_spec(&spec(8, 5, 4, 25), 7).expect("valid run");
        let b = MazeGame::new(8, 5, 4, 25, 7).expect("valid run");
        assert_eq!(a.maze(), b.maze());
        assert_eq!(a.collectibles(), b.collectibles());
    }
}

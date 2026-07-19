//! The windowed shell: one play screen on a ratgames `ScreenStack`.
//!
//! The ownership split follows the `ratgames::Screen` contract: the durable
//! session state — the run, the ladder progress, the config the rebakes read —
//! lives in [`Ctx`]; the *cached views* the frame draws from — the [`MazeScene`]
//! snapshot, the HUD banner, the win banner, and the play/cleared/won flow —
//! are local state on the [`PlayScreen`]. Input mutates the durable state
//! (`&mut Ctx`) and refreshes the screen's own views; rendering reads them.
//!
//! Arrow keys step the block one tile (one keydown, one move — the host's input
//! pump is edge-triggered per frame), `R` restarts the current level, `N`
//! re-deals it fresh, Enter advances off a cleared level (the ladder climbs
//! until the last rung, whose clear wins the run), and Esc quits.
//!
//! The maze itself is the [`MazeScene`] pixel layer; the HUD line and the
//! banners are `ShadowBanner` overlays baked through the config's glyph
//! source — a 32px Menlo raster in the shipped config, resolved once at
//! startup. (The in-maze digits are game pieces sized to their tiles, so
//! they stay bitmap sprites regardless — see [`MazeScene`].)

use mazegame_core::{Direction, MazeGame, MazeGameError};
use ratgames::{
    BannerStyle, GlyphSource, LevelProgress, OverlayLayer, PixelLayer, Screen, ScreenChange,
    ShadowBanner, ShadowBannerFactory, Size, UiInput, fill_placeholders,
};

use crate::config::{AppConfig, CopyConfig, MazeLevel, SceneConfig};
use crate::scene::MazeScene;

/// Where the shell stands: playing a level, holding a cleared-level banner
/// (Enter advances), or holding the run-won banner (Enter starts over).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Playing,
    LevelCleared,
    RunWon,
}

/// The durable session state threaded through the screen stack: the run, the
/// ladder it climbs, and the config the rebakes read. The drawable *views* —
/// the scene snapshot and the baked banners — are the [`PlayScreen`]'s, not the
/// context's.
pub struct Ctx {
    pub game: MazeGame,
    /// Progress through the ladder: `current()` is the rung in play, and
    /// clearing the last rung (`remaining() == 1`) wins the run.
    pub progress: LevelProgress,
    /// The ladder, easiest first (from config).
    pub levels: Vec<MazeLevel>,
    /// The glyph source the HUD and banners bake through (from
    /// `AppConfig::glyphs`), resolved once and shared.
    pub glyphs: Box<dyn GlyphSource>,
    pub text: BannerStyle,
    pub scene_cfg: SceneConfig,
    pub copy: CopyConfig,
    pub virtual_size: Size,
    /// Seed for the next deal; bumped per deal so mazes differ.
    pub next_seed: u64,
    pub quit: bool,
}

impl Ctx {
    /// A context whose first rung of `config`'s ladder is dealt and ready,
    /// seeded with `seed`, its banners to bake through `glyphs` (the caller
    /// resolves `config.glyphs` once — tests pass the deterministic bitmap
    /// instead). The [`PlayScreen`] bakes the initial views from this.
    ///
    /// # Errors
    /// [`MazeGameError`] if the first rung cannot deal (config validation
    /// makes this unreachable for a resolved config).
    pub fn new(
        config: &AppConfig,
        glyphs: Box<dyn GlyphSource>,
        seed: u64,
    ) -> Result<Self, MazeGameError> {
        let game = build_game(&config.levels[0], seed)?;
        Ok(Self {
            game,
            progress: LevelProgress::new(config.levels.len()),
            levels: config.levels.clone(),
            glyphs,
            text: config.text,
            scene_cfg: config.scene,
            copy: config.copy.clone(),
            virtual_size: config.engine.screen.size,
            next_seed: seed.wrapping_add(1),
            quit: false,
        })
    }

    /// The centred-banner factory in the app's style, over the durable glyph
    /// source and shadow.
    fn factory(&self) -> ShadowBannerFactory<'_> {
        ShadowBannerFactory::new(&*self.glyphs, self.text.shadow.style(), self.virtual_size)
    }
}

/// The playing screen: it owns the cached views (the maze scene snapshot, the
/// HUD line, the optional win/clear banner) and the play/cleared/won flow, and
/// refreshes them as input drives the run in the [`Ctx`]. Arrows step, Enter
/// climbs off a cleared level, `R` restarts the level, `N` re-deals it, Esc
/// quits.
pub struct PlayScreen {
    scene: MazeScene,
    hud: ShadowBanner,
    /// The centred banner over a cleared level or a won run.
    win: Option<ShadowBanner>,
    flow: Flow,
}

impl PlayScreen {
    /// The play screen for `ctx`'s current (freshly dealt) rung: snapshot the
    /// maze and bake the HUD.
    #[must_use]
    pub fn new(ctx: &Ctx) -> Self {
        let rung = ctx.levels[ctx.progress.current()];
        Self {
            scene: MazeScene::new(&ctx.scene_cfg, rung.tile_px, ctx.virtual_size, &ctx.game),
            hud: hud_line(ctx),
            win: None,
            flow: Flow::Playing,
        }
    }

    /// Step the block and refresh whatever the step changed: the scene on a
    /// move (or a collect — the glow passes to the next digit), the HUD on a
    /// collect, the flow and banner on a clear. A won level is inert, so late
    /// arrows change nothing.
    fn step(&mut self, direction: Direction, ctx: &mut Ctx) {
        let outcome = ctx.game.step(direction);
        if outcome.moved {
            self.scene.sync(&ctx.game);
        }
        if outcome.collected.is_some() {
            self.hud = hud_line(ctx);
        }
        if outcome.won {
            // Clearing the last rung (one left, counting this one) wins the
            // run; any earlier rung raises the level-clear banner. The rung
            // does not advance here — Enter climbs it.
            let line = if ctx.progress.remaining() == 1 {
                self.flow = Flow::RunWon;
                ctx.copy.win.clone()
            } else {
                self.flow = Flow::LevelCleared;
                fill_placeholders(
                    &ctx.copy.level_clear,
                    &[(ctx.progress.current() + 1).to_string()],
                )
            };
            self.win = Some(ctx.factory().centered(&line, ctx.text.banner_scale));
        }
    }

    /// Enter: climb off a cleared level, or start the ladder over from the
    /// run-won banner. Ignored while playing.
    fn advance(&mut self, ctx: &mut Ctx) {
        match self.flow {
            Flow::Playing => {}
            Flow::LevelCleared => {
                ctx.progress.advance();
                self.deal_level(ctx);
            }
            Flow::RunWon => {
                ctx.progress = LevelProgress::new(ctx.levels.len());
                self.deal_level(ctx);
            }
        }
    }

    /// Restart the current level: the same maze, the same digit placement,
    /// the block back at the start.
    fn restart_level(&mut self, ctx: &mut Ctx) {
        ctx.game.reset();
        self.scene.sync(&ctx.game);
        self.win = None;
        self.flow = Flow::Playing;
        self.hud = hud_line(ctx);
    }

    /// Deal the current rung fresh from the next seed. A rebuild under
    /// validated config cannot fail — but stay defensive and keep the current
    /// run if it ever did.
    fn deal_level(&mut self, ctx: &mut Ctx) {
        let rung = ctx.levels[ctx.progress.current()];
        let Ok(game) = build_game(&rung, ctx.next_seed) else {
            return;
        };
        ctx.next_seed = ctx.next_seed.wrapping_add(1);
        ctx.game = game;
        self.scene = MazeScene::new(&ctx.scene_cfg, rung.tile_px, ctx.virtual_size, &ctx.game);
        self.win = None;
        self.flow = Flow::Playing;
        self.hud = hud_line(ctx);
    }
}

impl Screen<Ctx> for PlayScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Up => self.step(Direction::Up, ctx),
            UiInput::Down => self.step(Direction::Down, ctx),
            UiInput::Left => self.step(Direction::Left, ctx),
            UiInput::Right => self.step(Direction::Right, ctx),
            UiInput::Char('r' | 'R') => self.restart_level(ctx),
            UiInput::Char('n' | 'N') => self.deal_level(ctx),
            UiInput::Confirm => self.advance(ctx),
            UiInput::Cancel => ctx.quit = true,
            _ => {}
        }
        ScreenChange::None
    }

    fn collect_layers<'a>(
        &'a self,
        _ctx: &'a Ctx,
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        world.push(&self.scene);
        overlays.push(&self.hud);
        if let Some(win) = &self.win {
            overlays.push(win);
        }
    }
}

/// Deal one rung: a maze of its shape and branching, carrying its digits.
fn build_game(level: &MazeLevel, seed: u64) -> Result<MazeGame, MazeGameError> {
    MazeGame::new(
        level.cells_w,
        level.cells_h,
        level.digits,
        level.branch_chance,
        seed,
    )
}

/// The HUD line for the run's current standing — level number, collected, total
/// — anchored at the config's HUD position and baked through its glyph source.
fn hud_line(ctx: &Ctx) -> ShadowBanner {
    let counts = [
        ctx.progress.current() + 1,
        ctx.game.collected(),
        ctx.game.total(),
    ];
    let line = fill_placeholders(&ctx.copy.hud, &counts.map(|count| count.to_string()));
    ShadowBannerFactory::new(&*ctx.glyphs, ctx.text.shadow.style(), ctx.virtual_size).at(
        &line,
        ctx.scene_cfg.hud_at,
        ctx.text.hud_scale,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazegame_core::{Maze, Phase, Tile};
    use ratgames::Bitmap8x8;

    /// A context on the bundled ladder (rung 0 in play) with the dealt game
    /// swapped for a hand corridor, and its play screen re-synced to it —
    /// deterministic movement, real config everywhere else. Banners bake
    /// through the bitmap source so no system font loads in tests (the raster
    /// look is pinned by the config tests).
    fn playing(numbers: Vec<(Tile, u8)>, exit: Tile) -> (Ctx, PlayScreen) {
        let config = AppConfig::resolve(None).expect("bundled config");
        let mut ctx = Ctx::new(&config, Box::new(Bitmap8x8), 1).expect("the first rung must deal");
        let maze = Maze::from_rows(&["#####", "#...#", "#####"]).expect("map");
        ctx.game = MazeGame::with_maze(maze, Tile::new(1, 1), exit, numbers).expect("valid game");
        let mut screen = PlayScreen::new(&ctx);
        // The hand game replaced the dealt one; resync the view to it.
        screen.scene = MazeScene::new(&config.scene, 10, ctx.virtual_size, &ctx.game);
        screen.hud = hud_line(&ctx);
        (ctx, screen)
    }

    fn play(screen: &mut PlayScreen, ctx: &mut Ctx, input: UiInput) {
        assert!(matches!(screen.handle(input, ctx), ScreenChange::None));
    }

    #[test]
    fn arrows_step_one_tile_and_bars_hold_the_block() {
        let (mut ctx, mut screen) = playing(vec![], Tile::new(3, 1));
        play(&mut screen, &mut ctx, UiInput::Up);
        assert_eq!(ctx.game.player(), Tile::new(1, 1), "the bar above is solid");
        play(&mut screen, &mut ctx, UiInput::Right);
        assert_eq!(ctx.game.player(), Tile::new(2, 1), "one keydown, one tile");
        play(&mut screen, &mut ctx, UiInput::Left);
        assert_eq!(ctx.game.player(), Tile::new(1, 1));
    }

    #[test]
    fn clearing_a_level_raises_the_banner_and_enter_climbs_the_ladder() {
        let (mut ctx, mut screen) = playing(vec![(Tile::new(2, 1), 7)], Tile::new(3, 1));
        assert!(screen.win.is_none());
        play(&mut screen, &mut ctx, UiInput::Right); // collects the 7
        assert_eq!(ctx.game.collected(), 1);
        play(&mut screen, &mut ctx, UiInput::Right); // enters the open exit
        assert_eq!(ctx.game.phase(), Phase::Won);
        assert_eq!(
            screen.flow,
            Flow::LevelCleared,
            "rung 1 of 7 is not the top"
        );
        assert!(screen.win.is_some(), "the clear banner is raised");

        play(&mut screen, &mut ctx, UiInput::Confirm);

        assert_eq!(ctx.progress.current(), 1, "Enter climbs to the next rung");
        assert_eq!(screen.flow, Flow::Playing);
        assert!(screen.win.is_none());
        assert_eq!(ctx.game.total(), ctx.levels[1].digits);
        let expected = ctx.levels[1].grid_tiles();
        assert_eq!(
            (ctx.game.maze().width(), ctx.game.maze().height()),
            expected,
            "the next rung deals its own shape"
        );
    }

    #[test]
    fn clearing_the_last_level_wins_the_run_and_enter_starts_over() {
        let (mut ctx, mut screen) = playing(vec![], Tile::new(2, 1));
        // Climb to the top rung.
        for _ in 1..ctx.levels.len() {
            ctx.progress.advance();
        }
        play(&mut screen, &mut ctx, UiInput::Right); // clear the corridor on the top rung
        assert_eq!(screen.flow, Flow::RunWon);
        assert!(screen.win.is_some(), "the run-won banner is raised");

        play(&mut screen, &mut ctx, UiInput::Confirm);

        assert_eq!(ctx.progress.current(), 0, "Enter starts the ladder over");
        assert_eq!(screen.flow, Flow::Playing);
        assert_eq!(ctx.game.total(), ctx.levels[0].digits);
    }

    #[test]
    fn r_restarts_the_same_level() {
        let (mut ctx, mut screen) = playing(vec![(Tile::new(2, 1), 7)], Tile::new(3, 1));
        play(&mut screen, &mut ctx, UiInput::Right); // collect the 7
        play(&mut screen, &mut ctx, UiInput::Right); // enter the open exit
        assert!(screen.win.is_some());
        let maze_before = ctx.game.maze().clone();

        play(&mut screen, &mut ctx, UiInput::Char('r'));

        assert_eq!(ctx.game.phase(), Phase::Playing);
        assert_eq!(screen.flow, Flow::Playing);
        assert!(screen.win.is_none(), "a restart clears the banner");
        assert_eq!(ctx.game.player(), Tile::new(1, 1), "back at the start");
        assert_eq!(ctx.game.collected(), 0, "the digits are restored");
        assert_eq!(ctx.game.total(), 1, "the same level, not a new deal");
        assert_eq!(ctx.game.maze(), &maze_before, "the same maze layout");
    }

    #[test]
    fn n_re_deals_the_current_rung_fresh() {
        let (mut ctx, mut screen) = playing(vec![], Tile::new(2, 1));
        play(&mut screen, &mut ctx, UiInput::Right); // win the corridor
        assert_eq!(ctx.game.phase(), Phase::Won);

        play(&mut screen, &mut ctx, UiInput::Char('n'));

        assert_eq!(ctx.game.phase(), Phase::Playing);
        assert_eq!(screen.flow, Flow::Playing);
        assert!(screen.win.is_none(), "a fresh deal clears the banner");
        assert_eq!(ctx.progress.current(), 0, "the same rung, freshly dealt");
        assert_eq!(ctx.game.total(), ctx.levels[0].digits);
        assert_eq!(ctx.game.player(), Tile::new(1, 1), "back at the start");
        let expected = ctx.levels[0].grid_tiles();
        assert_eq!(
            (ctx.game.maze().width(), ctx.game.maze().height()),
            expected,
            "the new maze has the rung's shape"
        );
    }

    #[test]
    fn escape_quits() {
        let (mut ctx, mut screen) = playing(vec![], Tile::new(3, 1));
        play(&mut screen, &mut ctx, UiInput::Cancel);
        assert!(ctx.quit);
    }
}

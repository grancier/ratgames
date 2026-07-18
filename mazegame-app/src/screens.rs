//! The windowed shell: one play screen on a ratgames `ScreenStack`.
//!
//! The durable run state lives in [`Ctx`]; the screen only routes input.
//! Arrow keys step the block one tile (one keydown, one move — the host's
//! input pump is edge-triggered per frame), `R` restarts the current level,
//! `N` re-deals it fresh, Enter advances off a cleared level (the ladder
//! climbs until the last rung, whose clear wins the run), and Esc quits.
//!
//! The maze itself is the [`MazeScene`] pixel layer; the HUD line and the
//! banners are `ShadowBanner` overlays baked through the config's glyph
//! source — a 32px Menlo raster in the shipped config, resolved once at
//! startup. (The in-maze digits are game pieces sized to their tiles, so
//! they stay bitmap sprites regardless — see [`MazeScene`].)

use mazegame_core::{Direction, MazeGame, MazeGameError};
use ratgames::{
    BannerStyle, GlyphSource, OverlayLayer, PixelLayer, Point, Screen, ScreenChange, ShadowBanner,
    ShadowBannerFactory, Size, UiInput, fill_placeholders,
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

/// The context threaded through the screen stack: the run, its drawable
/// scene, the baked overlay banners, and the config the rebakes read.
pub struct Ctx {
    pub game: MazeGame,
    pub scene: MazeScene,
    pub hud: ShadowBanner,
    /// The centred banner over a cleared level or a won run.
    pub win: Option<ShadowBanner>,
    pub flow: Flow,
    /// The rung of `levels` in play (0-based).
    pub level: usize,
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
    /// A context playing the first rung of `config`'s ladder, seeded with
    /// `seed`, its banners baked through `glyphs` (the caller resolves
    /// `config.glyphs` once — tests pass the deterministic bitmap instead).
    ///
    /// # Errors
    /// [`MazeGameError`] if the first rung cannot deal (config validation
    /// makes this unreachable for a resolved config).
    pub fn new(
        config: &AppConfig,
        glyphs: Box<dyn GlyphSource>,
        seed: u64,
    ) -> Result<Self, MazeGameError> {
        let virtual_size = config.engine.screen.size;
        let first = &config.levels[0];
        let game = build_game(first, seed)?;
        let scene = MazeScene::new(&config.scene, first.tile_px, virtual_size, &game);
        let hud = hud_banner(
            &*glyphs,
            &config.text,
            &config.copy,
            config.scene.hud_at,
            virtual_size,
            [1, game.collected(), game.total()],
        );
        Ok(Self {
            game,
            scene,
            hud,
            win: None,
            flow: Flow::Playing,
            level: 0,
            levels: config.levels.clone(),
            glyphs,
            text: config.text,
            scene_cfg: config.scene,
            copy: config.copy.clone(),
            virtual_size,
            next_seed: seed.wrapping_add(1),
            quit: false,
        })
    }

    /// Step the block and refresh whatever the step changed: the scene on a
    /// move (or a collect — the glow passes to the next digit), the HUD on a
    /// collect, the flow and banner on a clear. A won level is inert, so late
    /// arrows change nothing.
    fn step(&mut self, direction: Direction) {
        let outcome = self.game.step(direction);
        if outcome.moved {
            self.scene.sync(&self.game);
        }
        if outcome.collected.is_some() {
            self.rebake_hud();
        }
        if outcome.won {
            let line = if self.level + 1 == self.levels.len() {
                self.flow = Flow::RunWon;
                self.copy.win.clone()
            } else {
                self.flow = Flow::LevelCleared;
                fill_placeholders(&self.copy.level_clear, &[(self.level + 1).to_string()])
            };
            self.win = Some(self.factory().centered(&line, self.text.banner_scale));
        }
    }

    /// Enter: climb off a cleared level, or start the ladder over from the
    /// run-won banner. Ignored while playing.
    fn advance(&mut self) {
        match self.flow {
            Flow::Playing => {}
            Flow::LevelCleared => {
                self.level += 1;
                self.deal_level();
            }
            Flow::RunWon => {
                self.level = 0;
                self.deal_level();
            }
        }
    }

    /// Restart the current level: the same maze, the same digit placement,
    /// the block back at the start.
    fn restart_level(&mut self) {
        self.game.reset();
        self.scene.sync(&self.game);
        self.win = None;
        self.flow = Flow::Playing;
        self.rebake_hud();
    }

    /// Deal the current rung fresh from the next seed. A rebuild under
    /// validated config cannot fail — but stay defensive and keep the current
    /// run if it ever did.
    fn deal_level(&mut self) {
        let rung = self.levels[self.level];
        let Ok(game) = build_game(&rung, self.next_seed) else {
            return;
        };
        self.next_seed = self.next_seed.wrapping_add(1);
        self.game = game;
        self.scene = MazeScene::new(&self.scene_cfg, rung.tile_px, self.virtual_size, &self.game);
        self.win = None;
        self.flow = Flow::Playing;
        self.rebake_hud();
    }

    fn rebake_hud(&mut self) {
        self.hud = hud_banner(
            &*self.glyphs,
            &self.text,
            &self.copy,
            self.scene_cfg.hud_at,
            self.virtual_size,
            [self.level + 1, self.game.collected(), self.game.total()],
        );
    }

    fn factory(&self) -> ShadowBannerFactory<'_> {
        ShadowBannerFactory::new(&*self.glyphs, self.text.shadow.style(), self.virtual_size)
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

/// The HUD line, anchored at `at`, baked through `source`. `counts` fills the
/// copy's placeholders: `[level number, collected, total]`.
fn hud_banner(
    source: &dyn GlyphSource,
    text: &BannerStyle,
    copy: &CopyConfig,
    at: Point,
    virtual_size: Size,
    counts: [usize; 3],
) -> ShadowBanner {
    let line = fill_placeholders(&copy.hud, &counts.map(|count| count.to_string()));
    ShadowBannerFactory::new(source, text.shadow.style(), virtual_size).at(
        &line,
        at,
        text.hud_scale,
    )
}

/// The playing screen: arrows step, Enter climbs off a cleared level, `R`
/// restarts the level, `N` re-deals it, Esc quits.
pub struct PlayScreen;

impl Screen<Ctx> for PlayScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Up => ctx.step(Direction::Up),
            UiInput::Down => ctx.step(Direction::Down),
            UiInput::Left => ctx.step(Direction::Left),
            UiInput::Right => ctx.step(Direction::Right),
            UiInput::Char('r' | 'R') => ctx.restart_level(),
            UiInput::Char('n' | 'N') => ctx.deal_level(),
            UiInput::Confirm => ctx.advance(),
            UiInput::Cancel => ctx.quit = true,
            _ => {}
        }
        ScreenChange::None
    }

    fn collect_layers<'a>(
        &'a self,
        ctx: &'a Ctx,
        world: &mut Vec<&'a dyn PixelLayer>,
        overlays: &mut Vec<&'a dyn OverlayLayer>,
    ) {
        world.push(&ctx.scene);
        overlays.push(&ctx.hud);
        if let Some(win) = &ctx.win {
            overlays.push(win);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mazegame_core::{Maze, Phase, Tile};
    use ratgames::Bitmap8x8;

    /// A context on the bundled ladder (level 0 in play), then the level in
    /// play swapped for a hand corridor — deterministic movement, real config
    /// everywhere else. Banners bake through the bitmap source so no system
    /// font loads in tests (the raster look is pinned by the config tests).
    fn ctx(numbers: Vec<(Tile, u8)>, exit: Tile) -> Ctx {
        let config = AppConfig::resolve(None).expect("bundled config");
        let mut ctx = Ctx::new(&config, Box::new(Bitmap8x8), 1).expect("the first rung must deal");
        let maze = Maze::from_rows(&["#####", "#...#", "#####"]).expect("map");
        ctx.game = MazeGame::with_maze(maze, Tile::new(1, 1), exit, numbers).expect("valid game");
        ctx.scene = MazeScene::new(&config.scene, 10, ctx.virtual_size, &ctx.game);
        ctx.rebake_hud();
        ctx
    }

    fn play(ctx: &mut Ctx, input: UiInput) {
        assert!(matches!(PlayScreen.handle(input, ctx), ScreenChange::None));
    }

    #[test]
    fn arrows_step_one_tile_and_bars_hold_the_block() {
        let mut ctx = ctx(vec![], Tile::new(3, 1));
        play(&mut ctx, UiInput::Up);
        assert_eq!(ctx.game.player(), Tile::new(1, 1), "the bar above is solid");
        play(&mut ctx, UiInput::Right);
        assert_eq!(ctx.game.player(), Tile::new(2, 1), "one keydown, one tile");
        play(&mut ctx, UiInput::Left);
        assert_eq!(ctx.game.player(), Tile::new(1, 1));
    }

    #[test]
    fn clearing_a_level_raises_the_banner_and_enter_climbs_the_ladder() {
        let mut ctx = ctx(vec![(Tile::new(2, 1), 7)], Tile::new(3, 1));
        assert!(ctx.win.is_none());
        play(&mut ctx, UiInput::Right); // collects the 7
        assert_eq!(ctx.game.collected(), 1);
        play(&mut ctx, UiInput::Right); // enters the open exit
        assert_eq!(ctx.game.phase(), Phase::Won);
        assert_eq!(ctx.flow, Flow::LevelCleared, "rung 1 of 7 is not the top");
        assert!(ctx.win.is_some(), "the clear banner is raised");

        play(&mut ctx, UiInput::Confirm);

        assert_eq!(ctx.level, 1, "Enter climbs to the next rung");
        assert_eq!(ctx.flow, Flow::Playing);
        assert!(ctx.win.is_none());
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
        let mut ctx = ctx(vec![], Tile::new(2, 1));
        ctx.level = ctx.levels.len() - 1;
        play(&mut ctx, UiInput::Right); // clear the corridor on the top rung
        assert_eq!(ctx.flow, Flow::RunWon);
        assert!(ctx.win.is_some(), "the run-won banner is raised");

        play(&mut ctx, UiInput::Confirm);

        assert_eq!(ctx.level, 0, "Enter starts the ladder over");
        assert_eq!(ctx.flow, Flow::Playing);
        assert_eq!(ctx.game.total(), ctx.levels[0].digits);
    }

    #[test]
    fn r_restarts_the_same_level() {
        let mut ctx = ctx(vec![(Tile::new(2, 1), 7)], Tile::new(3, 1));
        play(&mut ctx, UiInput::Right); // collect the 7
        play(&mut ctx, UiInput::Right); // enter the open exit
        assert!(ctx.win.is_some());
        let maze_before = ctx.game.maze().clone();

        play(&mut ctx, UiInput::Char('r'));

        assert_eq!(ctx.game.phase(), Phase::Playing);
        assert_eq!(ctx.flow, Flow::Playing);
        assert!(ctx.win.is_none(), "a restart clears the banner");
        assert_eq!(ctx.game.player(), Tile::new(1, 1), "back at the start");
        assert_eq!(ctx.game.collected(), 0, "the digits are restored");
        assert_eq!(ctx.game.total(), 1, "the same level, not a new deal");
        assert_eq!(ctx.game.maze(), &maze_before, "the same maze layout");
    }

    #[test]
    fn n_re_deals_the_current_rung_fresh() {
        let mut ctx = ctx(vec![], Tile::new(2, 1));
        play(&mut ctx, UiInput::Right); // win the corridor
        assert_eq!(ctx.game.phase(), Phase::Won);

        play(&mut ctx, UiInput::Char('n'));

        assert_eq!(ctx.game.phase(), Phase::Playing);
        assert_eq!(ctx.flow, Flow::Playing);
        assert!(ctx.win.is_none(), "a fresh deal clears the banner");
        assert_eq!(ctx.level, 0, "the same rung, freshly dealt");
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
        let mut ctx = ctx(vec![], Tile::new(3, 1));
        play(&mut ctx, UiInput::Cancel);
        assert!(ctx.quit);
    }
}

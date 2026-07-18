//! The windowed shell: one play screen on a ratgames `ScreenStack`.
//!
//! The durable run state lives in [`Ctx`]; the screen only routes input.
//! Arrow keys step the block one tile (one keydown, one move — the host's
//! input pump is edge-triggered per frame), `R` restarts the current level,
//! `N` deals a fresh maze, and Esc quits. The maze itself is the [`MazeScene`] pixel layer; the HUD line and
//! the win banner are `ShadowBanner` overlays baked through the config's
//! glyph source — a 32px Menlo raster in the shipped config, resolved once at
//! startup. (The in-maze digits are game pieces squeezed into 10px tiles, so
//! they stay 8×8 bitmap sprites regardless — see [`MazeScene`].)

use mazegame_core::{Direction, MazeGame};
use ratgames::{
    BannerStyle, GlyphSource, OverlayLayer, PixelLayer, Point, Screen, ScreenChange, ShadowBanner,
    ShadowBannerFactory, Size, UiInput, fill_placeholders,
};

use crate::config::{AppConfig, CopyConfig, MazeConfig, SceneConfig};
use crate::scene::MazeScene;

/// The context threaded through the screen stack: the run, its drawable
/// scene, the baked overlay banners, and the config the rebakes read.
pub struct Ctx {
    pub game: MazeGame,
    pub scene: MazeScene,
    pub hud: ShadowBanner,
    /// The centred win banner, present once the run is won.
    pub win: Option<ShadowBanner>,
    /// The glyph source the HUD and win banners bake through (from
    /// `AppConfig::glyphs`), resolved once and shared.
    pub glyphs: Box<dyn GlyphSource>,
    pub text: BannerStyle,
    pub maze_cfg: MazeConfig,
    pub scene_cfg: SceneConfig,
    pub copy: CopyConfig,
    pub virtual_size: Size,
    /// Seed for the next `R` rebuild; bumped per deal so replays differ.
    pub next_seed: u64,
    pub quit: bool,
}

impl Ctx {
    /// A context over `game`, laid out and worded per `config`, with its
    /// banners baked through `glyphs` (the caller resolves
    /// `config.glyphs` once — tests pass the deterministic bitmap instead).
    #[must_use]
    pub fn new(
        config: &AppConfig,
        glyphs: Box<dyn GlyphSource>,
        game: MazeGame,
        next_seed: u64,
    ) -> Self {
        let virtual_size = config.engine.screen.size;
        let scene = MazeScene::new(&config.scene, &game);
        let hud = hud_banner(
            &*glyphs,
            &config.text,
            &config.copy,
            config.scene.hud_at,
            virtual_size,
            game.collected(),
            game.total(),
        );
        Self {
            game,
            scene,
            hud,
            win: None,
            glyphs,
            text: config.text,
            maze_cfg: config.maze,
            scene_cfg: config.scene,
            copy: config.copy.clone(),
            virtual_size,
            next_seed,
            quit: false,
        }
    }

    /// Step the block and refresh whatever the step changed: the scene on a
    /// move, the HUD on a collect, the banner on the win. A won run is inert,
    /// so late arrows change nothing.
    fn step(&mut self, direction: Direction) {
        let outcome = self.game.step(direction);
        if outcome.moved {
            self.scene.sync(&self.game);
        }
        if outcome.collected.is_some() {
            self.rebake_hud();
        }
        if outcome.won {
            self.win = Some(
                self.factory()
                    .centered(&self.copy.win, self.text.banner_scale),
            );
        }
    }

    /// Restart the current level: the same maze, the same digit placement,
    /// the block back at the start.
    fn restart_level(&mut self) {
        self.game.reset();
        self.scene.sync(&self.game);
        self.win = None;
        self.rebake_hud();
    }

    /// Deal a fresh maze of the configured shape from the next seed. The
    /// free-floor count of a perfect maze depends only on the cell counts, so
    /// a rebuild under validated config cannot fail — but stay defensive and
    /// keep the current run if it ever did.
    fn new_maze(&mut self) {
        let Ok(game) = MazeGame::new(
            self.maze_cfg.cells_w,
            self.maze_cfg.cells_h,
            self.maze_cfg.collectibles,
            self.next_seed,
        ) else {
            return;
        };
        self.next_seed = self.next_seed.wrapping_add(1);
        self.game = game;
        self.scene.sync(&self.game);
        self.win = None;
        self.rebake_hud();
    }

    fn rebake_hud(&mut self) {
        self.hud = hud_banner(
            &*self.glyphs,
            &self.text,
            &self.copy,
            self.scene_cfg.hud_at,
            self.virtual_size,
            self.game.collected(),
            self.game.total(),
        );
    }

    fn factory(&self) -> ShadowBannerFactory<'_> {
        ShadowBannerFactory::new(&*self.glyphs, self.text.shadow.style(), self.virtual_size)
    }
}

/// The collected-count HUD line, anchored at `at`, baked through `source`.
fn hud_banner(
    source: &dyn GlyphSource,
    text: &BannerStyle,
    copy: &CopyConfig,
    at: Point,
    virtual_size: Size,
    collected: usize,
    total: usize,
) -> ShadowBanner {
    let line = fill_placeholders(&copy.hud, &[collected.to_string(), total.to_string()]);
    ShadowBannerFactory::new(source, text.shadow.style(), virtual_size).at(
        &line,
        at,
        text.hud_scale,
    )
}

/// The playing screen: arrows step, `R` restarts the level, `N` deals a new
/// maze, Esc quits.
pub struct PlayScreen;

impl Screen<Ctx> for PlayScreen {
    fn handle(&mut self, input: UiInput, ctx: &mut Ctx) -> ScreenChange<Ctx> {
        match input {
            UiInput::Up => ctx.step(Direction::Up),
            UiInput::Down => ctx.step(Direction::Down),
            UiInput::Left => ctx.step(Direction::Left),
            UiInput::Right => ctx.step(Direction::Right),
            UiInput::Char('r' | 'R') => ctx.restart_level(),
            UiInput::Char('n' | 'N') => ctx.new_maze(),
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

    /// A corridor run under the bundled look: `#####` / `#..7#` with the exit
    /// right of the start — deterministic movement, real config everywhere
    /// else, except the banners bake through the bitmap source so no system
    /// font loads in tests (the raster look is pinned by the config tests).
    fn ctx(numbers: Vec<(Tile, u8)>, exit: Tile) -> Ctx {
        let config = AppConfig::resolve(None).expect("bundled config");
        let maze = Maze::from_rows(&["#####", "#...#", "#####"]).expect("map");
        let game = MazeGame::with_maze(maze, Tile::new(1, 1), exit, numbers).expect("valid game");
        Ctx::new(&config, Box::new(Bitmap8x8), game, 1)
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
    fn collecting_every_number_then_reaching_the_exit_wins() {
        let mut ctx = ctx(vec![(Tile::new(2, 1), 7)], Tile::new(3, 1));
        assert!(ctx.win.is_none());
        play(&mut ctx, UiInput::Right); // collects the 7
        assert_eq!(ctx.game.collected(), 1);
        play(&mut ctx, UiInput::Right); // enters the open exit
        assert_eq!(ctx.game.phase(), Phase::Won);
        assert!(ctx.win.is_some(), "the win banner is raised");
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
        assert!(ctx.win.is_none(), "a restart clears the banner");
        assert_eq!(ctx.game.player(), Tile::new(1, 1), "back at the start");
        assert_eq!(ctx.game.collected(), 0, "the digits are restored");
        assert_eq!(ctx.game.total(), 1, "the same level, not a new deal");
        assert_eq!(ctx.game.maze(), &maze_before, "the same maze layout");
    }

    #[test]
    fn n_deals_a_fresh_maze_of_the_configured_shape() {
        let mut ctx = ctx(vec![], Tile::new(2, 1));
        play(&mut ctx, UiInput::Right); // win the corridor
        assert_eq!(ctx.game.phase(), Phase::Won);

        play(&mut ctx, UiInput::Char('n'));

        assert_eq!(ctx.game.phase(), Phase::Playing);
        assert!(ctx.win.is_none(), "a fresh deal clears the banner");
        assert_eq!(ctx.game.total(), ctx.maze_cfg.collectibles);
        assert_eq!(ctx.game.player(), Tile::new(1, 1), "back at the start");
        let expected = (2 * ctx.maze_cfg.cells_w + 1, 2 * ctx.maze_cfg.cells_h + 1);
        assert_eq!(
            (ctx.game.maze().width(), ctx.game.maze().height()),
            expected,
            "the new maze has the configured shape"
        );
    }

    #[test]
    fn escape_quits() {
        let mut ctx = ctx(vec![], Tile::new(3, 1));
        play(&mut ctx, UiInput::Cancel);
        assert!(ctx.quit);
    }
}

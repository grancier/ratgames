//! Shared screen state and banner/input adapters.
use crate::config::{CopyConfig, LayoutConfig, PreparedDifficulty};
use mathgame_app::MathgameSession;
use ratgames::{
    AttractConfig, BannerContext, BannerStyle, CountdownConfig, FeedbackBeatConfig, GlyphSource,
    HighScores, InputContext, InputField, InputLine, JsonHighScoreStore, MeterBarConfig,
    OverlayLayer, RankRules, ShadowBannerFactory, Size,
};

use crate::scores;

/// The context threaded through the screen stack: the durable run state, the one
/// shared answer field (it owns a system font, so it lives here rather than per
/// screen), the pixel-art text style, the virtual screen size (for the banners to
/// recover the fit factor), and a quit flag the host loop watches.
pub struct Ctx {
    pub session: MathgameSession,
    pub input: InputField,
    pub text: BannerStyle,
    /// The glyph source the display-height banners and the reject cross render
    /// through (a 64px Menlo raster in the shipped config), resolved once and
    /// shared.
    pub glyphs: Box<dyn GlyphSource>,
    /// The optional smaller glyph source for body-height text (the HUD line,
    /// lists, board rows, readouts — the `hud_scale` family); `None` shares
    /// `glyphs`, the single-source look.
    pub hud_glyphs: Option<Box<dyn GlyphSource>>,
    pub feedback: FeedbackBeatConfig,
    /// The per-question timer bar's colours — a reusable `ratgames` meter-bar
    /// config; the bar's on-screen rect comes from the layout config.
    pub timer_bar: MeterBarConfig,
    /// The countdown config the Level Intro / Level Clear screens auto-advance on.
    pub interstitial: CountdownConfig,
    pub virtual_size: Size,
    /// The in-memory board, persisted through `store` as runs place.
    pub scores: HighScores,
    /// The persistence seam for `scores`, bound to the config path at startup.
    pub store: JsonHighScoreStore,
    /// The board's "top N" cap, applied when recording (a board never stores it).
    pub capacity: usize,
    /// Frames per second the host paces at — the unit for the question timer's
    /// budget and the per-second time bonus.
    pub frames_per_second: u32,
    /// Points per whole second left when a question is answered correctly.
    pub time_bonus_per_second: u32,
    /// Rank-based endings, proudest first; the result screen shows the first
    /// rank the finished run earns, or the plain win / game-over title.
    pub ranks: RankRules,
    /// How long the game-over CONTINUE? prompt holds before declining. (Whether a
    /// continue is offered at all is the session's policy: [`MathgameSession::can_continue`].)
    pub continue_prompt: CountdownConfig,
    /// Attract-mode timing: the title's idle trigger and the per-card hold.
    pub attract: AttractConfig,
    /// The selectable difficulties, in menu order; empty skips the select screen.
    pub difficulties: Vec<PreparedDifficulty>,
    /// Every user-facing string, from `copy.json` — no on-screen text is a Rust
    /// literal.
    pub copy: CopyConfig,
    /// Where every screen element sits, from `layout.json` — no position is a Rust
    /// literal.
    pub layout: LayoutConfig,
    /// The seed the next session rebuild draws its problem sequence from,
    /// bumped per rebuild so re-selecting a difficulty deals new problems.
    pub next_seed: u64,
    pub quit: bool,
}

impl Ctx {
    /// The glyph source for body-height text — the hud source when configured,
    /// else the shared banner source.
    fn hud_source(&self) -> &dyn GlyphSource {
        self.hud_glyphs.as_deref().unwrap_or(&*self.glyphs)
    }

    /// Record the finished run on the board and persist it — called once as a run
    /// ends, before the results and high-score screens read the board.
    pub(super) fn record_run(&mut self) {
        let name = self.session.profile().name().to_string();
        let points = self.session.run().score().points();
        scores::record_and_save(&self.store, &mut self.scores, &name, points, self.capacity);
    }

    /// Start the already-compiled campaign with a fresh deterministic sequence.
    pub(super) fn apply_difficulty(&mut self, index: usize) {
        let Some(preset) = self.difficulties.get(index) else {
            return;
        };
        self.session = preset.campaign.start(self.next_seed);
        self.next_seed = self.next_seed.wrapping_add(1);
    }
}

/// Build a [`ShadowBannerFactory`] in the app's pixel-art style: `source`'s glyphs
/// (a 32px Menlo raster in the shipped config) with the config's em-relative drop
/// shadow, anchored to the virtual screen. The reusable banner composition lives
/// in `ratgames`; this only supplies the app's glyph source and shadow. Callers
/// pass the per-banner magnification (the app's `banner_scale` / `hud_scale`).
fn banner_factory(
    source: &dyn GlyphSource,
    style: BannerStyle,
    virtual_size: Size,
) -> ShadowBannerFactory<'_> {
    ShadowBannerFactory::new(source, style.shadow.style(), virtual_size)
}

/// The app's screen context hands `ratgames` screens its banner factory, so a
/// generic pixel-art screen that re-bakes on interaction (e.g. [`ratgames::ChoiceScreen`])
/// composites in the app's own style. Delegates to the free `banner_factory`.
impl BannerContext for Ctx {
    fn banner_factory(&self) -> ShadowBannerFactory<'_> {
        banner_factory(&*self.glyphs, self.text, self.virtual_size)
    }

    fn hud_factory(&self) -> ShadowBannerFactory<'_> {
        banner_factory(self.hud_source(), self.text, self.virtual_size)
    }
}

/// The context likewise hands `ratgames` screens its one durable input field
/// through the text-entry seam: the editable line for editing / submit, the
/// drawn field for rendering.
impl InputContext for Ctx {
    fn input_line(&mut self) -> &mut InputLine {
        self.input.line_mut()
    }

    fn input_overlay(&self) -> &dyn OverlayLayer {
        &self.input
    }
}

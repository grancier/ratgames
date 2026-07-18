//! ratgames — a small retro-game presentation toolkit.
//!
//! The design splits cleanly along one axis: **pixel-art world** vs
//! **device-space overlay**.
//!
//! * [`Surface`] is the one blittable buffer type; [`Sprite`] is the one unit
//!   of pixel art; [`Color`] / [`palette`] are the one colour vocabulary.
//! * [`PixelLayer`]s ([`Marquee`], [`RoomView`], …) draw into a low-resolution
//!   virtual [`Surface`]. [`Presentation`] upscales it by an integer factor and
//!   letterboxes it into the window.
//! * [`OverlayLayer`]s ([`InputField`], …) draw into the window afterwards, in
//!   device pixels, so anti-aliased UI text ([`SystemFont`]) is never
//!   pixel-scaled.
//!
//! Every tunable — sizes, colours, the input font — lives in [`Config`]; nothing
//! downstream hardcodes a literal. New capabilities are new layer
//! implementations; the compositor is closed for modification.

pub mod color;
pub mod config;
pub mod copy;
pub mod font;
pub mod geometry;
pub mod glyph;
#[cfg(feature = "minifb")]
pub mod host;
pub mod input;
pub mod marquee;
pub mod overlay;
pub mod placard;
pub mod present;
pub mod scene;
pub mod session;
pub mod sprite;
pub mod surface;
pub mod text;
pub mod theme;
pub mod ui;

pub use color::{Color, palette};
pub use config::{
    BorderConfig, Config, ConfigError, ConfigFileError, ConfigSource, DeviceClass, FontConfig,
    FontFamily, FontSource, FontStretch, FontStyle, FontWeight, GlyphSourceConfig, InputConfig,
    InputLayout, LevelConfig, LevelLoadError, MarqueeConfig, ScreenConfig, WindowConfig,
    load_config_file, load_levels_dir, parse_config_flag, take_levels_flag,
};
pub use copy::fill_placeholders;
pub use font::{FontError, LineMetrics, RasterGlyph, SystemFont};
pub use geometry::{Point, Rect, Size};
pub use glyph::{Bitmap8x8, GlyphMask, GlyphSource, RasterGlyphSource};
#[cfg(feature = "minifb")]
pub use host::{HostError, MinifbHost};
pub use input::{InputField, InputLine};
pub use marquee::Marquee;
pub use overlay::TextStyle;
pub use placard::Placard;
pub use present::{OverlayLayer, PixelLayer, Presentation};
pub use scene::{Direction, Overworld, Room, RoomId, RoomMap, RoomView, Transition};
pub use session::{
    AttemptOutcome, AttractCard, AttractConfig, AttractLoop, AwardOutcome, BannerContext,
    BoardFooter, BoardLine, Campaign, CampaignError, Challenge, ChallengeAnswer,
    ChallengeResolution, ChallengeScreen, ChallengeView, ChoiceScreen, ContinueExit,
    ContinuePrompt, ContinueRules, GameRules, GameRulesError, GameRun, GradedAttempt,
    HighScoreBoard, HighScoreBoardSpec, HighScoreEntry, HighScoreLayout, HighScoreStoreError,
    HighScores, InputContext, JsonHighScoreStore, LevelGoal, LevelGoalError, LevelOutcome,
    LevelProgress, LevelSpec, LevelSpecError, Lives, OneUpRules, PlacedRow, PlayerProfile,
    PromptExit, PromptScreen, RankRule, RankRules, RankRulesError, Run, RunPhase, RunTally, Score,
    ScoresConfig, ScoringRules, ScoringRulesError, Screen, ScreenChange, ScreenStack, StreakRules,
    TextEntryExit, TextEntryScreen, TimedCard, TimedCardExit, accuracy_percent,
};
pub use sprite::{Sprite, SpriteError};
pub use surface::{BlitOptions, QuarterTurns, Surface};
pub use text::{BigText, Footprint, Ink, TextColors};
pub use theme::Theme;
pub use ui::{
    Align, AnswerMode, AnswerModeError, Axis, BannerAnchor, BannerColumn, BannerStyle, Blink,
    BlinkConfig, Borders, ChoiceList, Constraint, Countdown, CountdownConfig, FeedbackBeat,
    FeedbackBeatConfig, FeedbackBeatLayers, Flash, Label, Menu, MenuView, MeterBar, MeterBarConfig,
    MultipleChoice, Panel, Paragraph, ShadowBanner, ShadowBannerFactory, ShadowConfig,
    ShadowLength, ShadowStyle, TimedGauge, UiInput, bake_drop_shadow, split, stacked_rects,
    wrap_lines,
};

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
pub mod font;
pub mod game;
pub mod geometry;
pub mod glyph;
#[cfg(feature = "minifb")]
pub mod host;
pub mod input;
pub mod marquee;
pub mod overlay;
pub mod placard;
pub mod present;
pub mod quiz;
pub mod scene;
pub mod session;
pub mod sprite;
pub mod surface;
pub mod text;
pub mod theme;
pub mod ui;

pub use color::{Color, palette};
pub use config::{
    BannerConfig, BorderConfig, Config, ConfigError, ConfigSource, DeviceClass, FlashConfig,
    FontConfig, FontFamily, FontSource, FontStretch, FontStyle, FontWeight, GlyphSourceConfig,
    InputConfig, InputLayout, MarqueeConfig, QuizConfig, ScreenConfig, WindowConfig,
    parse_config_flag,
};
pub use font::{FontError, LineMetrics, RasterGlyph, SystemFont};
pub use game::MathGame;
pub use geometry::{Point, Rect, Size};
pub use glyph::{Bitmap8x8, GlyphMask, GlyphSource, RasterGlyphSource};
#[cfg(feature = "minifb")]
pub use host::{HostError, MinifbHost};
pub use input::{InputField, InputLine};
pub use marquee::Marquee;
pub use overlay::TextStyle;
pub use placard::Placard;
pub use present::{OverlayLayer, PixelLayer, Presentation};
pub use quiz::{Outcome, Phase, Question, Quiz};
pub use scene::{Direction, Overworld, Room, RoomId, RoomMap, RoomView, Transition};
pub use session::{
    HighScoreEntry, HighScoreStoreError, HighScores, JsonHighScoreStore, LevelGoal, LevelGoalError,
    LevelOutcome, LevelProgress, Lives, PlayerProfile, Run, RunPhase, Score, Screen, ScreenChange,
    ScreenStack,
};
pub use sprite::{Sprite, SpriteError};
pub use surface::Surface;
pub use text::{BigText, Footprint, Ink, TextColors};
pub use theme::Theme;
pub use ui::{
    Align, Axis, BannerAnchor, Blink, Borders, Constraint, Flash, Label, Menu, MenuView,
    MultipleChoice, Panel, Paragraph, ShadowBanner, ShadowLength, ShadowStyle, UiInput,
    bake_drop_shadow, split, stacked_rects, wrap_lines,
};

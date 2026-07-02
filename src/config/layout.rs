//! Window, screen, marquee, and input-panel configuration.

use crate::color::Color;
use crate::geometry::{Point, Rect, Size};
use crate::sprite::Sprite;
use crate::text::{BigText, TextColors};
use crate::theme::Theme;

use super::defaults::DEFAULT_STRINGS;
use super::{
    ConfigError, DeviceClass, FontConfig, GlyphSourceConfig, guard_footprint, validate_glyph_source,
};

/// Physical window.
///
/// The initial size is chosen responsively: [`device`](Self::device) selects a
/// [`DeviceClass`] preset, and [`width`](Self::width)/[`height`](Self::height)
/// override it per axis when set. Read the result with [`size`](Self::size).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct WindowConfig {
    pub title: String,
    /// The device-class preset that seeds the initial size.
    pub device: DeviceClass,
    /// Explicit width override (device px); falls back to the `device` preset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    /// Explicit height override (device px); falls back to the `device` preset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    pub target_fps: usize,
    pub resizable: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: DEFAULT_STRINGS.window.title.clone(),
            device: DeviceClass::Desktop,
            width: None,
            height: None,
            target_fps: 60,
            resizable: true,
        }
    }
}

impl WindowConfig {
    /// The resolved initial window size (device px): the explicit
    /// [`width`](Self::width)/[`height`](Self::height) where given, otherwise the
    /// [`device`](Self::device) preset, per axis. The default is Desktop —
    /// 1280×720.
    #[must_use]
    pub fn size(&self) -> Size {
        let preset = self.device.preset_size();
        Size::new(
            self.width.unwrap_or(preset.w),
            self.height.unwrap_or(preset.h),
        )
    }
}

/// The low-resolution virtual screen the pixel world composes into.
///
/// [`size`](Self::size) is the base (Desktop) surface; the optional per-class
/// sizes let the surface switch as the window is resized across a breakpoint
/// (see [`size_for`](Self::size_for)). The defaults are chosen to integer-fill
/// each [`DeviceClass`] preset exactly: 320×180 → 1280×720 at 4×, 180×320 →
/// 360×640 at 2×, 192×256 → 768×1024 at 4×.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ScreenConfig {
    pub backdrop: Color,
    pub letterbox: Color,
    /// Crisp-clip floor for the integer upscale: the virtual screen is never
    /// presented below this factor (a smaller window clips instead of blurring).
    pub min_scale: u32,
    /// Base (Desktop) virtual screen size. Declared before the per-class
    /// sub-tables so all sub-tables follow this struct's scalar fields in TOML.
    pub size: Size,
    /// Virtual screen used at the Mobile breakpoint; falls back to `size` if unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mobile_size: Option<Size>,
    /// Virtual screen used at the Tablet breakpoint; falls back to `size` if unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tablet_size: Option<Size>,
}

impl Default for ScreenConfig {
    fn default() -> Self {
        let theme = Theme::default();
        Self {
            backdrop: theme.background,
            letterbox: theme.letterbox,
            min_scale: 1,
            size: Size::new(320, 180), // 16:9, fills 1280×720 at 4×
            mobile_size: Some(Size::new(180, 320)), // 9:16, fills 360×640 at 2×
            tablet_size: Some(Size::new(192, 256)), // 3:4, fills 768×1024 at 4×
        }
    }
}

impl ScreenConfig {
    /// The virtual screen size for a device class: the per-class override if set,
    /// otherwise the base [`size`](Self::size). The presentation is rebuilt with
    /// this when the live window width crosses into a new [`DeviceClass`].
    #[must_use]
    pub fn size_for(&self, class: DeviceClass) -> Size {
        match class {
            DeviceClass::Mobile => self.mobile_size.unwrap_or(self.size),
            DeviceClass::Tablet => self.tablet_size.unwrap_or(self.size),
            DeviceClass::Desktop => self.size,
        }
    }
}

/// The scrolling big-text banner.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct MarqueeConfig {
    pub text_scale: u32,
    pub tracking: u32,
    pub shadow_depth: u32,
    /// Outline thickness around the letters, in source pixels (`0` = none).
    pub outline_px: u32,
    pub gap: u32,
    pub speed: u32,
    pub colors: TextColors,
    /// Which glyph source the letters rasterise through.
    pub glyph_source: GlyphSourceConfig,
}

impl Default for MarqueeConfig {
    fn default() -> Self {
        Self {
            text_scale: 6,
            tracking: 1,
            shadow_depth: 3,
            outline_px: 1,
            gap: 14,
            speed: 2,
            colors: TextColors::default(),
            glyph_source: GlyphSourceConfig::Bitmap8x8,
        }
    }
}

impl MarqueeConfig {
    /// Bake `text` into a scrolling-banner sprite through the configured glyph
    /// source and style.
    ///
    /// # Errors
    /// Returns [`ConfigError::Font`] if a raster glyph source's font cannot be
    /// loaded, or [`ConfigError::SpriteTooLarge`] if the banner would exceed the
    /// pre-allocation size limits.
    pub fn text_sprite(&self, text: &str) -> Result<Sprite, ConfigError> {
        // Bound the glyph source at this boundary too: a banner can be baked from
        // a Config that never went through validate() (e.g. Config::default()),
        // and an oversized cell_px must be rejected before it is rasterised.
        validate_glyph_source(&self.glyph_source, "marquee")?;
        let source = self.glyph_source.resolve()?;
        let big = BigText::new(self.text_scale)
            .tracking(self.tracking)
            .shadow_depth(self.shadow_depth)
            .outline(self.outline_px)
            .gap(self.gap)
            .colors(self.colors);
        guard_footprint(&big, &*source, text)?;
        Ok(big.build_with(&*source, text))
    }
}

/// The bottom input panel: a nested border framing an anti-aliased text line.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct InputConfig {
    /// Fraction of the window height the panel occupies (`0.0..=1.0`).
    pub height_fraction: f32,
    /// Outer margin from the panel edge to the first border, in device pixels.
    pub margin_px: u32,
    /// Inner padding from the innermost border to the text, in device pixels.
    pub padding_px: u32,
    /// Text caret width, in device pixels.
    pub caret_width_px: u32,
    pub background_color: Color,
    pub text_color: Color,
    /// Colour of the fixed prompt drawn before the editable answer. Defaults to
    /// the accent (a tinted label), distinct from the answer's `text_color`.
    pub prompt_color: Color,
    pub border: BorderConfig,
    pub font: FontConfig,
}

impl Default for InputConfig {
    fn default() -> Self {
        let theme = Theme::default();
        Self {
            height_fraction: 0.15,
            margin_px: 8,
            padding_px: 8,
            caret_width_px: 2,
            background_color: theme.panel,
            text_color: theme.ink,
            prompt_color: theme.accent, // tinted label, distinct from the answer
            border: BorderConfig::default(),
            font: FontConfig::default(),
        }
    }
}

/// A nested (concentric) line border.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BorderConfig {
    pub color: Color,
    /// Thickness of each line, in device pixels.
    pub line_thickness_px: u32,
    /// Number of concentric lines ("2 lines all around").
    pub line_count: u32,
    /// Gap between adjacent lines, in device pixels.
    pub line_gap_px: u32,
}

impl Default for BorderConfig {
    fn default() -> Self {
        Self {
            color: Theme::default().accent,
            line_thickness_px: 2,
            line_count: 2,
            line_gap_px: 3,
        }
    }
}

/// Resolved geometry of the input panel, all derived from [`InputConfig`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputLayout {
    pub panel: Rect,
    /// Concentric border rects, outermost first.
    pub borders: Vec<Rect>,
    /// The rect glyphs are drawn (and clipped) into.
    pub text_area: Rect,
}

impl InputConfig {
    /// Compute the panel, border, and text rects for a given window size.
    /// Pure and literal-free — everything comes from `self`.
    #[must_use]
    pub fn layout(&self, window: Size) -> InputLayout {
        let panel_h = (f64::from(window.h) * f64::from(self.height_fraction)).round() as u32;
        let panel = Rect::new(
            Point::new(0, window.h.saturating_sub(panel_h) as i32),
            Size::new(window.w, panel_h),
        );

        let step = self.border.line_thickness_px + self.border.line_gap_px;
        let mut borders = Vec::with_capacity(self.border.line_count as usize);
        let mut inset = self.margin_px;
        for _ in 0..self.border.line_count {
            borders.push(inset_rect(panel, inset));
            inset += step;
        }

        // Text sits inside the innermost line: drop the trailing gap, add padding.
        let text_inset = inset.saturating_sub(self.border.line_gap_px) + self.padding_px;
        InputLayout {
            panel,
            borders,
            text_area: inset_rect(panel, text_inset),
        }
    }
}

/// Shrink a rect inward by `by` pixels on every side.
fn inset_rect(r: Rect, by: u32) -> Rect {
    let d = by as i32;
    Rect::new(
        Point::new(r.origin.x + d, r.origin.y + d),
        Size::new(
            r.size.w.saturating_sub(2 * by),
            r.size.h.saturating_sub(2 * by),
        ),
    )
}

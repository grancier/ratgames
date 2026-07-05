//! [`Blink`] — a sprite that flashes a fixed number of times over the viewport.
//!
//! The arcade "flash this symbol N times" mechanic: a rejected-input X, a
//! warning glyph, a blinking "1UP". It draws one [`Sprite`] — centred (or
//! anchored) and integer-scaled to the game viewport exactly like a
//! [`ShadowBanner`](super::ShadowBanner) — but only during the "lit" part of each
//! blink. The sprite is drawn in its own colours, so bake it in the colour you
//! want (a red reject cross, say).
//!
//! `Blink` owns the blink *pattern* (how many blinks, and the on / off frames of
//! each) but not a clock: the caller pumps one frame per [`advance`](Blink::advance)
//! from its own frame source (e.g. `Screen::tick`) and stops when
//! [`is_done`](Blink::is_done), the same division of labour as [`Flash`](super::Flash).
//! This keeps it reusable across any pacing and unit-testable with no timer.

use crate::geometry::{Rect, Size};
use crate::present::OverlayLayer;
use crate::sprite::Sprite;
use crate::surface::Surface;

use super::shadow_banner::{BannerAnchor, place_in_viewport};

/// A sprite that blinks a fixed number of times over the game viewport. Built
/// with a sprite and an anchor; the blink pattern and scale are set with the
/// builders. The sprite draws in its own colours.
#[derive(Debug, Clone)]
pub struct Blink {
    sprite: Sprite,
    scale_mult: u32,
    anchor: BannerAnchor,
    virtual_size: Size,
    blinks: u32,
    on_frames: u32,
    off_frames: u32,
    frame: u32,
}

impl Blink {
    /// A sprite blinking within a viewport sized against `virtual_size`, anchored
    /// by `anchor`. Defaults: `1×` scale, three blinks of six on / six off frames.
    /// Tune with the builders below.
    #[must_use]
    pub fn new(sprite: Sprite, anchor: BannerAnchor, virtual_size: Size) -> Self {
        Self {
            sprite,
            scale_mult: 1,
            anchor,
            virtual_size,
            blinks: 3,
            on_frames: 6,
            off_frames: 6,
            frame: 0,
        }
    }

    /// Set the device-scale multiplier (the sprite is drawn at `mult × fit`, so it
    /// magnifies with the window like a pixel layer). Clamped to at least 1.
    #[must_use]
    pub fn scale(mut self, mult: u32) -> Self {
        self.scale_mult = mult.max(1);
        self
    }

    /// Set the blink pattern: `blinks` on/off cycles, each `on_frames` lit then
    /// `off_frames` dark.
    #[must_use]
    pub fn pattern(mut self, blinks: u32, on_frames: u32, off_frames: u32) -> Self {
        self.blinks = blinks;
        self.on_frames = on_frames;
        self.off_frames = off_frames;
        self
    }

    /// Advance one frame. Saturates at the end, so over-pumping stays done.
    pub fn advance(&mut self) {
        if self.frame < self.total() {
            self.frame += 1;
        }
    }

    /// Whether every blink has played (past this point nothing is drawn).
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.frame >= self.total()
    }

    /// Total frames across all blinks.
    fn total(&self) -> u32 {
        self.blinks * (self.on_frames + self.off_frames)
    }

    /// Whether the sprite is drawn this frame: within the `on_frames` window of a
    /// blink cycle, and not yet finished.
    fn is_lit(&self) -> bool {
        let cycle = (self.on_frames + self.off_frames).max(1);
        !self.is_done() && self.frame % cycle < self.on_frames
    }
}

impl OverlayLayer for Blink {
    fn render(&self, window: &mut Surface, viewport: Rect) {
        if !self.is_lit() {
            return;
        }
        let (origin, scale) = place_in_viewport(
            viewport,
            self.virtual_size,
            self.sprite.size(),
            self.scale_mult,
            self.anchor,
        );
        window.draw_sprite_scaled(&self.sprite, scale, origin);
    }
}

/// A serde config for a [`Blink`]'s pattern: how many blinks, and the lit / dark
/// frames of each cycle. A game carries the product value (a reject-cross flash, a
/// warning blink) in its config and stamps it onto a freshly-built [`Blink`] with
/// [`apply`](Self::apply) — the reusable *type* lives here, the *value* lives in
/// the game's config, like [`CountdownConfig`](super::CountdownConfig).
///
/// `blinks == 0` (or `on_frames == 0`) is a valid "never lit" pattern — nothing is
/// drawn — so there is nothing to validate, mirroring `CountdownConfig`'s zero hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BlinkConfig {
    /// Number of on/off blink cycles.
    pub blinks: u32,
    /// Frames the sprite is lit in each cycle.
    pub on_frames: u32,
    /// Frames the sprite is dark in each cycle.
    pub off_frames: u32,
}

impl Default for BlinkConfig {
    fn default() -> Self {
        // Matches Blink::new's built-in pattern: three blinks of six on / six off.
        Self {
            blinks: 3,
            on_frames: 6,
            off_frames: 6,
        }
    }
}

impl BlinkConfig {
    /// Stamp this pattern onto `blink`, returning it — a builder pass-through onto
    /// [`Blink::pattern`], so the caller sets the sprite / anchor / scale and the
    /// *timing* comes from config.
    #[must_use]
    pub fn apply(&self, blink: Blink) -> Blink {
        blink.pattern(self.blinks, self.on_frames, self.off_frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Color, palette};
    use crate::geometry::Point;

    /// A 1x1 opaque sprite — enough to probe placement and lit/dark drawing.
    fn dot() -> Sprite {
        let mut sprite = Sprite::new(Size::new(1, 1));
        sprite.set(Point::ORIGIN, palette::FILL);
        sprite
    }

    fn blink() -> Blink {
        Blink::new(dot(), BannerAnchor::Center, Size::new(10, 10))
    }

    #[test]
    fn it_lights_the_configured_number_of_on_off_cycles_then_finishes() {
        // cycle = on(2) + off(2) = 4; total = 3 * 4 = 12.
        let mut b = blink().pattern(3, 2, 2);
        let mut lit = Vec::new();
        for _ in 0..b.total() {
            lit.push(b.is_lit());
            b.advance();
        }
        assert!(b.is_done());
        assert_eq!(
            lit,
            vec![
                true, true, false, false, // blink 1
                true, true, false, false, // blink 2
                true, true, false, false, // blink 3
            ]
        );
    }

    #[test]
    fn advance_saturates_and_stays_done_and_dark() {
        let mut b = blink().pattern(1, 1, 1); // total = 2
        for _ in 0..5 {
            b.advance();
        }
        assert!(b.is_done());
        assert!(!b.is_lit());
    }

    #[test]
    fn it_draws_the_sprite_only_while_lit() {
        let vs = Size::new(4, 4);
        let vp = Rect::new(Point::ORIGIN, vs);
        // dot() is a palette::FILL pixel: a lit frame draws it, a dark one doesn't.
        let mut b = Blink::new(dot(), BannerAnchor::Center, vs).pattern(1, 1, 1);

        let mut lit_window = Surface::new(vs, Color::rgb(0, 0, 0));
        b.render(&mut lit_window, vp);
        assert!(
            lit_window
                .as_slice()
                .iter()
                .any(|&w| w == palette::FILL.packed()),
            "a lit frame draws the sprite"
        );

        b.advance(); // into the off frame
        let mut dark_window = Surface::new(vs, Color::rgb(0, 0, 0));
        b.render(&mut dark_window, vp);
        assert!(
            dark_window
                .as_slice()
                .iter()
                .all(|&w| w != palette::FILL.packed()),
            "a dark frame draws nothing"
        );
    }

    #[test]
    fn config_stamps_its_pattern_and_round_trips() {
        // apply() sets the pattern: total frames = blinks * (on + off) = 2*(3+4) = 14.
        let config = BlinkConfig {
            blinks: 2,
            on_frames: 3,
            off_frames: 4,
        };
        let mut b = config.apply(blink());
        let mut frames = 0;
        while !b.is_done() {
            b.advance();
            frames += 1;
        }
        assert_eq!(frames, 14);

        let text = serde_json::to_string(&config).expect("serialize");
        let parsed: BlinkConfig = serde_json::from_str(&text).expect("deserialize");
        assert_eq!(parsed, config);
        // A sparse config fills every field from the default.
        let defaulted: BlinkConfig = serde_json::from_str("{}").expect("deserialize empty");
        assert_eq!(defaulted, BlinkConfig::default());
    }
}

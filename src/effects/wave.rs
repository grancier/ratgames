//! [`TextWave`] — a row of glyphs that ripples up and back down.
//!
//! Each character is baked once into a full-cell [`Sprite`] through a
//! [`GlyphSource`] (so every letter shares the source's baseline grid), laid out
//! on the pen by the glyph metrics, and drawn each frame at an integer vertical
//! offset from the [`wave_offsets`] cascade — the lead letter rises first, each
//! following letter a fixed distance behind, all falling back in turn. The offset
//! rounds to whole pixels, so the motion stays crisp under
//! [`Presentation`](crate::Presentation)'s integer upscale.
//!
//! The rasterising and blitting are plain ratgames — a [`GlyphSource`] and a
//! [`Surface`] — so the effect needs no external rasteriser and stays integer.

use crate::color::Color;
use crate::geometry::{Point, Size};
use crate::glyph::{GlyphMask, GlyphSource};
use crate::present::PixelLayer;
use crate::sprite::Sprite;
use crate::surface::Surface;

/// Frames for a letter to travel from rest to the top of its arc.
const WAVE_RISE_FRAMES: f32 = 60.0;
/// Frames for a full up-and-back-down cycle.
const WAVE_CYCLE_FRAMES: f32 = WAVE_RISE_FRAMES * 2.0;
/// How far behind the previous letter each letter starts, as a fraction of the
/// rise — the cascade's spacing.
const LETTER_START_DISTANCE_FRACTION: f32 = 0.05;
/// Default motion frames advanced per [`TextWave::advance`] call.
const DEFAULT_SPEED: f32 = 5.0;
/// Extra source pixels the layout inserts between adjacent letters.
const DEFAULT_TRACKING: u32 = 1;

/// One baked letter: its full-cell ink sprite, the pen origin it sits at (source
/// pixels), and the glyph's left side bearing.
#[derive(Debug, Clone)]
struct Letter {
    sprite: Sprite,
    pen_x: u32,
    x_offset: i32,
}

/// A pixel-art [`PixelLayer`] that ripples a line of text. Build it with
/// [`new`](Self::new) from any [`GlyphSource`], tune with [`scale`](Self::scale)
/// / [`speed`](Self::speed), and [`advance`](Self::advance) it once per frame.
/// The run centres horizontally and rests along the bottom of the surface it is
/// drawn into, rising toward the top through the wave.
#[derive(Debug, Clone)]
pub struct TextWave {
    letters: Vec<Letter>,
    /// Total pen advance of the run, source pixels (for centring).
    run_width: u32,
    /// Common glyph cell height, source pixels.
    cell_height: u32,
    scale: u32,
    speed: f32,
    frame: u32,
}

impl TextWave {
    /// Bake `text` through `source` in `ink`. Letters lay out on the pen by their
    /// glyph advance plus a default tracking gap; the wave and integer scale are
    /// applied when it is drawn.
    #[must_use]
    pub fn new(source: &dyn GlyphSource, text: &str, ink: Color) -> Self {
        let cell_height = source.cell_height().max(1);
        let mut letters = Vec::new();
        let mut pen = 0u32;
        for (index, ch) in text.chars().enumerate() {
            if index > 0 {
                pen += DEFAULT_TRACKING;
            }
            let mask = source.glyph(ch);
            letters.push(Letter {
                sprite: cell_sprite(&mask, ink),
                pen_x: pen,
                x_offset: mask.x_offset,
            });
            pen += mask.advance;
        }
        Self {
            letters,
            run_width: pen,
            cell_height,
            scale: 1,
            speed: sanitized_speed(DEFAULT_SPEED),
            frame: 0,
        }
    }

    /// Integer magnification of the baked glyphs (treated as at least 1).
    #[must_use]
    pub fn scale(mut self, scale: u32) -> Self {
        self.scale = scale.max(1);
        self
    }

    /// Motion frames advanced per [`advance`](Self::advance) — higher ripples
    /// faster. Non-positive or non-finite values freeze the wave.
    #[must_use]
    pub fn speed(mut self, speed: f32) -> Self {
        self.speed = sanitized_speed(speed);
        self
    }

    /// Step the animation one frame.
    pub fn advance(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// The current frame counter — handy for tests and deterministic captures.
    #[must_use]
    pub fn frame(&self) -> u32 {
        self.frame
    }
}

impl PixelLayer for TextWave {
    fn render(&self, screen: &mut Surface) {
        let size = screen.size();
        let run_px = self.run_width * self.scale;
        let letter_px = self.cell_height * self.scale;
        let start_x = (size.w as i32 - run_px as i32) / 2;
        // Rest along the bottom; the wave lifts each letter up toward the top.
        let rest_top = (size.h as i32 - letter_px as i32).max(0);
        let travel = rest_top as f32;
        let offsets = wave_offsets(self.letters.len(), self.frame, travel, self.speed);
        for (letter, offset) in self.letters.iter().zip(&offsets) {
            let x = start_x + (letter.pen_x as i32 + letter.x_offset) * self.scale as i32;
            let y = rest_top + offset.round() as i32;
            screen.draw_sprite_scaled(&letter.sprite, self.scale, Point::new(x, y));
        }
    }
}

/// Build a full-cell ink [`Sprite`] from `mask` — every cell pixel, ink or
/// transparent — so letters keep their baseline placement across the run (unlike
/// [`GlyphMask::to_sprite`], which crops to the ink and drops the vertical
/// placement a run needs). A zero-extent mask (e.g. a space) yields a 1×1
/// transparent sprite that draws nothing.
fn cell_sprite(mask: &GlyphMask, ink: Color) -> Sprite {
    if mask.width == 0 || mask.height == 0 {
        return Sprite::new(Size::new(1, 1));
    }
    let mut sprite = Sprite::new(Size::new(mask.width, mask.height));
    for y in 0..mask.height {
        for x in 0..mask.width {
            if mask.get(x, y) {
                sprite.set(Point::new(x as i32, y as i32), ink);
            }
        }
    }
    sprite
}

/// The per-letter vertical offset (source pixels, `-travel..=0`, negative is up)
/// at animation `frame`: each letter rises over the first half of its cycle and
/// falls over the second, delayed one [`LETTER_START_DISTANCE_FRACTION`] of the
/// rise behind the letter before it.
#[must_use]
fn wave_offsets(count: usize, frame: u32, travel: f32, speed: f32) -> Vec<f32> {
    wave_offsets_at_time(count, frame as f32, travel, speed)
}

/// [`wave_offsets`] at a continuous `frame`, factored out so the timing is
/// testable without stepping a whole animation.
#[must_use]
fn wave_offsets_at_time(count: usize, frame: f32, travel: f32, speed: f32) -> Vec<f32> {
    let motion_frame = frame.max(0.0) * sanitized_speed(speed);
    let travel = travel.max(0.0);
    let start_delay_frames = WAVE_RISE_FRAMES * LETTER_START_DISTANCE_FRACTION;

    (0..count)
        .map(|i| {
            let local_frame = motion_frame - i as f32 * start_delay_frames;
            if local_frame <= 0.0 {
                return 0.0;
            }
            let progress = (local_frame % WAVE_CYCLE_FRAMES) / WAVE_CYCLE_FRAMES;
            let upward_progress = if progress <= 0.5 {
                progress * 2.0
            } else {
                (1.0 - progress) * 2.0
            };
            -travel * upward_progress
        })
        .collect()
}

/// A usable positive speed, or `0.0` (a frozen wave) for anything non-finite or
/// non-positive.
#[must_use]
fn sanitized_speed(speed: f32) -> f32 {
    if speed.is_finite() && speed > 0.0 {
        speed
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::glyph::Bitmap8x8;

    const RED: Color = Color::rgb(255, 0, 0);
    const BG: Color = Color::rgb(0, 0, 0);

    // ---- timing ----

    #[test]
    fn offsets_delay_each_letters_start() {
        let initial = wave_offsets_at_time(4, 0.0, 100.0, 1.0);
        let after_first_distance = wave_offsets_at_time(
            4,
            WAVE_RISE_FRAMES * LETTER_START_DISTANCE_FRACTION,
            100.0,
            1.0,
        );

        assert_eq!(
            initial,
            vec![0.0, 0.0, 0.0, 0.0],
            "everything rests at frame 0"
        );
        assert!(
            (after_first_distance[0] + 5.0).abs() < 0.01,
            "letter zero is 5% up when the next letter starts"
        );
        assert!(
            after_first_distance[1].abs() < 0.01,
            "letter one still waits at the bottom"
        );
    }

    #[test]
    fn offsets_move_each_letter_to_the_top_then_back() {
        let at_top = wave_offsets_at_time(1, WAVE_RISE_FRAMES, 100.0, 1.0);
        let back = wave_offsets_at_time(1, WAVE_CYCLE_FRAMES, 100.0, 1.0);
        assert!((at_top[0] + 100.0).abs() < 0.01, "travels all the way up");
        assert!(back[0].abs() < 0.01, "returns to the bottom");
    }

    #[test]
    fn speed_scales_elapsed_time_not_the_cascade_spacing() {
        let slow_frame = WAVE_RISE_FRAMES * LETTER_START_DISTANCE_FRACTION;
        let slow = wave_offsets_at_time(2, slow_frame, 100.0, 1.0);
        let fast = wave_offsets_at_time(2, slow_frame / 5.0, 100.0, 5.0);
        assert!((slow[0] - fast[0]).abs() < 0.01, "5x speed, 1/5 the frames");
        assert!((slow[1] - fast[1]).abs() < 0.01, "same distance threshold");
    }

    #[test]
    fn a_frozen_speed_never_moves() {
        let wave = TextWave::new(&Bitmap8x8, "HI", RED).speed(0.0);
        let offsets = wave_offsets(2, 999, 100.0, wave.speed);
        assert_eq!(offsets, vec![0.0, 0.0]);
    }

    // ---- rendering ----

    /// The topmost row holding any ink, or `None` for a blank surface.
    fn top_ink_row(surface: &Surface) -> Option<u32> {
        let size = surface.size();
        (0..size.h).find(|&y| {
            (0..size.w).any(|x| surface.as_slice()[(y * size.w + x) as usize] == RED.packed())
        })
    }

    #[test]
    fn at_rest_the_run_sits_along_the_bottom() {
        // cell height 8, surface 24 tall -> the resting run occupies rows 16..24.
        let wave = TextWave::new(&Bitmap8x8, "WAVE", RED);
        let mut screen = Surface::new(Size::new(64, 24), BG);
        wave.render(&mut screen);
        let top = top_ink_row(&screen).expect("the text draws ink");
        assert!(
            top >= 16,
            "every letter rests in the bottom 8px band, got {top}"
        );
    }

    #[test]
    fn advancing_lifts_the_lead_letter_out_of_the_bottom_band() {
        // Default speed 5: 12 advances -> motion_frame 60 -> the lead letter is
        // at the top of its arc.
        let mut wave = TextWave::new(&Bitmap8x8, "WAVE", RED);
        for _ in 0..12 {
            wave.advance();
        }
        assert_eq!(wave.frame(), 12);
        let mut screen = Surface::new(Size::new(64, 24), BG);
        wave.render(&mut screen);
        let top = top_ink_row(&screen).expect("the text still draws ink");
        assert!(
            top < 16,
            "the lead letter has risen out of the bottom band, got {top}"
        );
    }

    #[test]
    fn scale_magnifies_the_glyphs() {
        let plain = TextWave::new(&Bitmap8x8, "A", RED);
        let big = TextWave::new(&Bitmap8x8, "A", RED).scale(3);
        let count_ink = |wave: &TextWave| {
            let mut screen = Surface::new(Size::new(64, 48), BG);
            wave.render(&mut screen);
            screen
                .as_slice()
                .iter()
                .filter(|&&w| w == RED.packed())
                .count()
        };
        assert!(
            count_ink(&big) > count_ink(&plain) * 4,
            "a 3x scale covers ~9x the pixels"
        );
    }

    #[test]
    fn empty_text_draws_nothing_without_panicking() {
        let wave = TextWave::new(&Bitmap8x8, "", RED);
        let mut screen = Surface::new(Size::new(16, 16), BG);
        wave.render(&mut screen);
        assert!(top_ink_row(&screen).is_none());
    }
}

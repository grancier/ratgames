//! [`Surface`] — an owned, blittable pixel buffer.
//!
//! One type backs both the low-resolution virtual screen and the physical
//! window framebuffer, so compositing, sprite blitting, and the integer upscale
//! all live here (DRY) rather than being re-derived per call site.

use crate::color::Color;
use crate::geometry::{Point, Rect, Size};
use crate::sprite::Sprite;

/// A rectangular buffer of `0x00RRGGBB` words (framebuffer-ready).
#[derive(Debug, Clone)]
pub struct Surface {
    size: Size,
    buf: Vec<u32>,
}

impl Surface {
    /// A surface filled with `fill`.
    #[must_use]
    pub fn new(size: Size, fill: Color) -> Self {
        Self {
            size,
            buf: vec![fill.packed(); size.area()],
        }
    }

    #[must_use]
    pub fn size(&self) -> Size {
        self.size
    }

    /// The raw buffer, for handing to the window backend.
    #[must_use]
    pub fn as_slice(&self) -> &[u32] {
        &self.buf
    }

    /// Overwrite every pixel with `color`.
    pub fn fill(&mut self, color: Color) {
        self.buf.fill(color.packed());
    }

    /// Write one pixel, skipping transparent colours and out-of-bounds points.
    pub fn set(&mut self, p: Point, color: Color) {
        if color.is_visible() && self.size.contains(p) {
            let i = self.index(p);
            self.buf[i] = color.packed();
        }
    }

    /// Composite `sprite` with its top-left at `at`, honouring transparency and
    /// clipping to the surface bounds.
    pub fn draw_sprite(&mut self, sprite: &Sprite, at: Point) {
        let s = sprite.size();
        for sy in 0..s.h {
            for sx in 0..s.w {
                let src = Point::new(sx as i32, sy as i32);
                self.set(Point::new(at.x + src.x, at.y + src.y), sprite.get(src));
            }
        }
    }

    /// Nearest-neighbour integer upscale of `src` into this surface: every
    /// source pixel becomes a `scale × scale` block whose top-left lands at
    /// `dst`. Clipped to bounds. `scale` is treated as at least 1.
    ///
    /// `src` is opaque (a screen, not a sprite), so its pixels are copied
    /// verbatim without a transparency test — the hot path of the frame.
    pub fn draw_upscaled(&mut self, src: &Surface, scale: u32, dst: Point) {
        let scale = scale.max(1);
        let s = src.size;
        for sy in 0..s.h {
            for sx in 0..s.w {
                let word = src.buf[sy as usize * s.w as usize + sx as usize];
                let base_x = dst.x + (sx * scale) as i32;
                let base_y = dst.y + (sy * scale) as i32;
                for ry in 0..scale {
                    let y = base_y + ry as i32;
                    if y < 0 || y >= self.size.h as i32 {
                        continue;
                    }
                    for rx in 0..scale {
                        let x = base_x + rx as i32;
                        if x < 0 || x >= self.size.w as i32 {
                            continue;
                        }
                        self.buf[y as usize * self.size.w as usize + x as usize] = word;
                    }
                }
            }
        }
    }

    /// Blit `sprite` at integer `scale` — each source pixel a `scale × scale`
    /// block — with its top-left at `at`, honouring transparency and clipping to
    /// bounds. `scale` is treated as at least 1.
    ///
    /// The device-space counterpart of [`draw_sprite`](Self::draw_sprite): where
    /// [`draw_upscaled`](Self::draw_upscaled) magnifies the whole (opaque) virtual
    /// screen, this magnifies one transparent sprite, for pixel-art UI composited
    /// *after* the upscale (e.g. a banner in an overlay) whose block size is
    /// chosen in device pixels.
    pub fn draw_sprite_scaled(&mut self, sprite: &Sprite, scale: u32, at: Point) {
        self.blit_scaled(sprite, scale, at, None);
    }

    /// Like [`draw_sprite_scaled`](Self::draw_sprite_scaled), but every opaque
    /// pixel is drawn in `color`, ignoring the sprite's own colours — a solid
    /// silhouette, for a drop shadow behind the real sprite.
    pub fn draw_sprite_silhouette(&mut self, sprite: &Sprite, scale: u32, at: Point, color: Color) {
        self.blit_scaled(sprite, scale, at, Some(color));
    }

    /// Shared body of the scaled sprite blits: expand each opaque source pixel to
    /// a `scale × scale` block, drawing either the pixel's own colour or `tint`
    /// when overriding it (a silhouette). [`set`](Self::set) clips and skips
    /// transparency per pixel.
    fn blit_scaled(&mut self, sprite: &Sprite, scale: u32, at: Point, tint: Option<Color>) {
        let scale = scale.max(1) as i32;
        let s = sprite.size();
        for sy in 0..s.h as i32 {
            for sx in 0..s.w as i32 {
                let src = sprite.get(Point::new(sx, sy));
                if !src.is_visible() {
                    continue;
                }
                let color = tint.unwrap_or(src);
                let base_x = at.x + sx * scale;
                let base_y = at.y + sy * scale;
                for ry in 0..scale {
                    for rx in 0..scale {
                        self.set(Point::new(base_x + rx, base_y + ry), color);
                    }
                }
            }
        }
    }

    /// Fill an axis-aligned rectangle with an opaque colour, clipped to bounds.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        if !color.is_visible() {
            return;
        }
        let word = color.packed();
        let x0 = rect.origin.x.max(0);
        let y0 = rect.origin.y.max(0);
        let x1 = rect.right().min(self.size.w as i32);
        let y1 = rect.bottom().min(self.size.h as i32);
        let mut y = y0;
        while y < y1 {
            let row = y as usize * self.size.w as usize;
            let mut x = x0;
            while x < x1 {
                self.buf[row + x as usize] = word;
                x += 1;
            }
            y += 1;
        }
    }

    /// Draw `thickness`-thick edges just inside `rect` (a hollow rectangle).
    pub fn draw_rect_outline(&mut self, rect: Rect, color: Color, thickness: u32) {
        let t = thickness as i32;
        let Rect { origin, size } = rect;
        self.fill_rect(Rect::new(origin, Size::new(size.w, thickness)), color); // top
        self.fill_rect(
            Rect::new(
                Point::new(origin.x, rect.bottom() - t),
                Size::new(size.w, thickness),
            ),
            color,
        ); // bottom
        self.fill_rect(Rect::new(origin, Size::new(thickness, size.h)), color); // left
        self.fill_rect(
            Rect::new(
                Point::new(rect.right() - t, origin.y),
                Size::new(thickness, size.h),
            ),
            color,
        ); // right
    }

    /// Alpha-composite `color` over the pixel at `p` by `coverage` (`0..=255`).
    /// The anti-aliased path — distinct from the 1-bit [`set`](Self::set).
    pub fn blend(&mut self, p: Point, color: Color, coverage: u8) {
        if coverage == 0 || !self.size.contains(p) {
            return;
        }
        let i = self.index(p);
        self.buf[i] = blend_word(self.buf[i], color.packed(), u32::from(coverage));
    }

    /// Alpha-composite `color` over every pixel in `rect`, clipped to bounds, at
    /// the coverage carried by `color`'s own alpha — the rectangle counterpart of
    /// [`blend`](Self::blend) and the translucent sibling of
    /// [`fill_rect`](Self::fill_rect). A zero-alpha colour is a no-op, so a tint
    /// can fade to nothing. Backs the `Flash` overlay's full-viewport wash.
    pub fn blend_rect(&mut self, rect: Rect, color: Color) {
        let coverage = color.alpha();
        if coverage == 0 {
            return;
        }
        let a = u32::from(coverage);
        let src = color.packed();
        let x0 = rect.origin.x.max(0);
        let y0 = rect.origin.y.max(0);
        let x1 = rect.right().min(self.size.w as i32);
        let y1 = rect.bottom().min(self.size.h as i32);
        let mut y = y0;
        while y < y1 {
            let row = y as usize * self.size.w as usize;
            let mut x = x0;
            while x < x1 {
                let i = row + x as usize;
                self.buf[i] = blend_word(self.buf[i], src, a);
                x += 1;
            }
            y += 1;
        }
    }

    fn index(&self, p: Point) -> usize {
        p.y as usize * self.size.w as usize + p.x as usize
    }
}

/// Source-over composite of `src` onto `dst` (both `0x00RRGGBB` words) at
/// `a/255` coverage, rounded. The shared core of [`Surface::blend`] and
/// [`Surface::blend_rect`], so the compositing formula lives in one place.
fn blend_word(dst: u32, src: u32, a: u32) -> u32 {
    let inv = 255 - a;
    let mix = |shift: u32| {
        let s = (src >> shift) & 0xFF;
        let d = (dst >> shift) & 0xFF;
        (s * a + d * inv + 127) / 255
    };
    (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word_at(s: &Surface, x: u32, y: u32) -> u32 {
        s.as_slice()[y as usize * s.size().w as usize + x as usize]
    }

    #[test]
    fn fill_and_set_and_transparency() {
        let bg = Color::rgb(0, 0, 0);
        let mut s = Surface::new(Size::new(2, 2), bg);
        assert_eq!(word_at(&s, 0, 0), bg.packed());

        let red = Color::rgb(255, 0, 0);
        s.set(Point::new(1, 1), red);
        assert_eq!(word_at(&s, 1, 1), red.packed());

        // Transparent write is ignored; so is out of bounds.
        s.set(Point::new(0, 0), Color::TRANSPARENT);
        assert_eq!(word_at(&s, 0, 0), bg.packed());
        s.set(Point::new(9, 9), red); // must not panic
    }

    #[test]
    fn draw_sprite_composites_and_clips() {
        let mut s = Surface::new(Size::new(3, 3), Color::rgb(0, 0, 0));
        let mut spr = Sprite::new(Size::new(2, 2));
        let red = Color::rgb(255, 0, 0);
        spr.set(Point::new(0, 0), red); // rest transparent
        // Place straddling the bottom-right corner: only (2,2) lands.
        s.draw_sprite(&spr, Point::new(2, 2));
        assert_eq!(word_at(&s, 2, 2), red.packed());
        assert_eq!(word_at(&s, 0, 0), Color::rgb(0, 0, 0).packed());
    }

    #[test]
    fn draw_upscaled_expands_each_pixel_to_a_block() {
        let mut src = Surface::new(Size::new(2, 1), Color::rgb(0, 0, 0));
        let red = Color::rgb(255, 0, 0);
        src.set(Point::new(0, 0), red);

        let mut dst = Surface::new(Size::new(4, 2), Color::rgb(0, 0, 0));
        dst.draw_upscaled(&src, 2, Point::ORIGIN);

        // The red source pixel becomes the top-left 2x2 block.
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(word_at(&dst, x, y), red.packed());
        }
        // The transparent-black source pixel becomes the next 2x2 block.
        assert_eq!(word_at(&dst, 2, 0), Color::rgb(0, 0, 0).packed());
    }

    #[test]
    fn draw_sprite_scaled_blocks_and_skips_transparent() {
        let mut spr = Sprite::new(Size::new(2, 1));
        let red = Color::rgb(255, 0, 0);
        spr.set(Point::new(0, 0), red); // (1,0) stays transparent
        let bg = Color::rgb(0, 0, 0);
        let mut s = Surface::new(Size::new(4, 2), bg);
        s.draw_sprite_scaled(&spr, 2, Point::ORIGIN);

        // The opaque pixel becomes a 2x2 block.
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(word_at(&s, x, y), red.packed());
        }
        // The transparent pixel's block is left untouched.
        assert_eq!(word_at(&s, 2, 0), bg.packed());
        assert_eq!(word_at(&s, 3, 1), bg.packed());
    }

    #[test]
    fn draw_sprite_silhouette_recolours_opaque_pixels() {
        let mut spr = Sprite::new(Size::new(2, 1));
        spr.set(Point::new(0, 0), Color::rgb(255, 0, 0)); // opaque; (1,0) transparent
        let bg = Color::rgb(0, 0, 0);
        let yellow = Color::rgb(255, 255, 0);
        let mut s = Surface::new(Size::new(4, 2), bg);
        s.draw_sprite_silhouette(&spr, 2, Point::ORIGIN, yellow);

        // The opaque pixel's block is drawn in the silhouette colour, not red.
        assert_eq!(word_at(&s, 0, 0), yellow.packed());
        assert_eq!(word_at(&s, 1, 1), yellow.packed());
        assert_eq!(word_at(&s, 0, 0), yellow.packed());
        // The transparent pixel stays background.
        assert_eq!(word_at(&s, 2, 0), bg.packed());
    }

    #[test]
    fn draw_sprite_scaled_clips_at_the_edge() {
        let mut spr = Sprite::new(Size::new(2, 2));
        let red = Color::rgb(255, 0, 0);
        for y in 0..2 {
            for x in 0..2 {
                spr.set(Point::new(x, y), red);
            }
        }
        let mut s = Surface::new(Size::new(3, 3), Color::rgb(0, 0, 0));
        // A 2x block at (2,2): only the corner block's top-left lands; the rest
        // clips off the right/bottom edges without panicking.
        s.draw_sprite_scaled(&spr, 2, Point::new(2, 2));
        assert_eq!(word_at(&s, 2, 2), red.packed());
    }

    #[test]
    fn fill_rect_clips_to_bounds() {
        let mut s = Surface::new(Size::new(3, 3), Color::rgb(0, 0, 0));
        let red = Color::rgb(255, 0, 0);
        s.fill_rect(Rect::new(Point::new(2, 2), Size::new(5, 5)), red);
        assert_eq!(word_at(&s, 2, 2), red.packed());
        assert_eq!(word_at(&s, 0, 0), Color::rgb(0, 0, 0).packed());
    }

    #[test]
    fn rect_outline_draws_edges_not_interior() {
        let mut s = Surface::new(Size::new(5, 5), Color::rgb(0, 0, 0));
        let blue = Color::rgb(0, 0, 255);
        s.draw_rect_outline(Rect::from_size(Size::new(5, 5)), blue, 1);
        assert_eq!(word_at(&s, 0, 0), blue.packed()); // corner
        assert_eq!(word_at(&s, 2, 0), blue.packed()); // top edge
        assert_eq!(word_at(&s, 2, 2), Color::rgb(0, 0, 0).packed()); // interior untouched
    }

    #[test]
    fn blend_composites_by_coverage() {
        let mut s = Surface::new(Size::new(1, 1), Color::rgb(0, 0, 0));
        let white = Color::rgb(255, 255, 255);
        s.blend(Point::ORIGIN, white, 0);
        assert_eq!(word_at(&s, 0, 0), 0); // no-op
        s.blend(Point::ORIGIN, white, 128);
        assert_eq!(word_at(&s, 0, 0), 0x0080_8080); // ~50% over black
        s.blend(Point::ORIGIN, white, 255);
        assert_eq!(word_at(&s, 0, 0), 0x00FF_FFFF); // full
    }

    #[test]
    fn blend_rect_composites_a_region_by_the_colour_alpha_and_clips() {
        let mut s = Surface::new(Size::new(3, 3), Color::rgb(0, 0, 0));
        let white_half = Color::argb(128, 255, 255, 255);
        // A 2x2 region at (1,1) that overruns the bottom-right: it clips, no panic.
        s.blend_rect(Rect::new(Point::new(1, 1), Size::new(5, 5)), white_half);
        assert_eq!(word_at(&s, 1, 1), 0x0080_8080); // ~50% white over black
        assert_eq!(word_at(&s, 2, 2), 0x0080_8080);
        assert_eq!(word_at(&s, 0, 0), Color::rgb(0, 0, 0).packed()); // outside untouched
    }

    #[test]
    fn blend_rect_is_a_noop_at_zero_alpha() {
        let bg = Color::rgb(10, 20, 30);
        let mut s = Surface::new(Size::new(2, 2), bg);
        s.blend_rect(
            Rect::from_size(Size::new(2, 2)),
            Color::argb(0, 255, 255, 255),
        );
        assert_eq!(word_at(&s, 0, 0), bg.packed()); // a fully-transparent wash draws nothing
    }

    #[test]
    fn blend_rect_at_full_alpha_replaces_the_region() {
        let mut s = Surface::new(Size::new(2, 2), Color::rgb(0, 0, 0));
        s.blend_rect(
            Rect::from_size(Size::new(2, 2)),
            Color::argb(255, 255, 0, 0),
        );
        assert_eq!(word_at(&s, 0, 0), Color::rgb(255, 0, 0).packed());
        assert_eq!(word_at(&s, 1, 1), Color::rgb(255, 0, 0).packed());
    }
}

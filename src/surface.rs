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
        // Clip once per axis (this is the hot path of the frame), then write
        // each source pixel's block as row spans.
        let scale = i64::from(scale.max(1));
        let (dx, dy) = (i64::from(dst.x), i64::from(dst.y));
        let (sw, sh) = (i64::from(self.size.w), i64::from(self.size.h));
        let sx_range = visible_blocks(dx, scale, i64::from(src.size.w), sw);
        let sy_range = visible_blocks(dy, scale, i64::from(src.size.h), sh);
        let stride = src.size.w as usize;
        for sy in sy_range {
            let top = dy + sy * scale;
            let (y0, y1) = (top.max(0), (top + scale).min(sh));
            for sx in sx_range.clone() {
                let word = src.buf[sy as usize * stride + sx as usize];
                let left = dx + sx * scale;
                let (x0, x1) = (left.max(0) as usize, (left + scale).min(sw) as usize);
                for y in y0..y1 {
                    self.span_mut(y as usize, x0, x1).fill(word);
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
        self.draw_sprite_ex(
            sprite,
            at,
            BlitOptions {
                scale: scale.max(1),
                ..BlitOptions::default()
            },
        );
    }

    /// Like [`draw_sprite_scaled`](Self::draw_sprite_scaled), but every opaque
    /// pixel is drawn in `color`, ignoring the sprite's own colours — a solid
    /// silhouette, for a drop shadow behind the real sprite.
    pub fn draw_sprite_silhouette(&mut self, sprite: &Sprite, scale: u32, at: Point, color: Color) {
        self.draw_sprite_ex(
            sprite,
            at,
            BlitOptions {
                scale: scale.max(1),
                tint: Some(color),
                ..BlitOptions::default()
            },
        );
    }

    /// Composite a sprite — or a `region` of it — with mirroring, quarter-turn
    /// rotation, and integer scaling: the full-control blit behind tile and
    /// sprite-sheet rendering. The transform stays crisp by construction: the
    /// region (clamped to the sprite's bounds) is mirrored per
    /// `flip_x`/`flip_y`, the mirrored image is turned clockwise by `turns`,
    /// and each resulting pixel becomes a `scale × scale` block whose top-left
    /// lands at `at`. Transparent source pixels leave the destination
    /// untouched, exactly like [`draw_sprite`](Self::draw_sprite); a `scale`
    /// of `0` draws nothing.
    ///
    /// The options collapse to one of eight dihedral transforms, precomputed
    /// once ([`SpriteTransform`]); destination clipping is computed once
    /// before the loops; and opaque blocks are written as row spans — the
    /// per-pixel path carries no branching on the options and no bounds
    /// checks.
    pub fn draw_sprite_ex(&mut self, sprite: &Sprite, at: Point, options: BlitOptions) {
        if options.scale == 0 {
            return;
        }
        let Some(region) = clamped_region(sprite.size(), options.region) else {
            return;
        };
        let transform = SpriteTransform::new(region.size, &options);

        // Destination clipping, once: which output pixels have any visible
        // part, given each is a scale-wide block. All block arithmetic is
        // i64 so large sprites, scales, and positions cannot overflow.
        let scale = i64::from(options.scale);
        let (ax, ay) = (i64::from(at.x), i64::from(at.y));
        let (sw, sh) = (i64::from(self.size.w), i64::from(self.size.h));
        let ox_range = visible_blocks(ax, scale, i64::from(transform.out_size.w), sw);
        let oy_range = visible_blocks(ay, scale, i64::from(transform.out_size.h), sh);

        // The region is pre-clamped, so source reads index the pixel slice
        // directly (the crate-internal fast path — `Sprite` otherwise hides
        // its layout).
        let pixels = sprite.pixels();
        let stride = sprite.size().w as usize;
        let (region_x, region_y) = (region.origin.x as usize, region.origin.y as usize);

        let mut emit = |ox: i64, oy: i64| {
            let (sx, sy) = transform.source(ox as i32, oy as i32);
            let color = pixels[(region_y + sy as usize) * stride + region_x + sx as usize];
            if !color.is_visible() {
                return;
            }
            let color = options.tint.unwrap_or(color);
            self.fill_block(ax + ox * scale, ay + oy * scale, scale, color);
        };

        // Walk the output so SOURCE reads stay row-major: the axis-swapping
        // transforms (90° / 270°) read along output columns, so those iterate
        // columns outermost.
        if transform.swaps_axes {
            for ox in ox_range {
                for oy in oy_range.clone() {
                    emit(ox, oy);
                }
            }
        } else {
            for oy in oy_range {
                for ox in ox_range.clone() {
                    emit(ox, oy);
                }
            }
        }
    }

    /// Write one opaque `scale × scale` block with its top-left at
    /// `(left, top)`, clipped to the surface — the emission half of
    /// [`draw_sprite_ex`](Self::draw_sprite_ex), kept span-based.
    #[inline]
    fn fill_block(&mut self, left: i64, top: i64, scale: i64, color: Color) {
        let (sw, sh) = (i64::from(self.size.w), i64::from(self.size.h));
        let (x0, x1) = (left.max(0) as usize, (left + scale).min(sw) as usize);
        for y in top.max(0)..(top + scale).min(sh) {
            self.set_span(y as usize, x0, x1, color);
        }
    }

    /// One horizontal run `[x0, x1)` of row `y` as a mutable word slice — the
    /// span primitive the fills, blends, and blits build on. Bounds are the
    /// caller's contract.
    #[inline]
    fn span_mut(&mut self, y: usize, x0: usize, x1: usize) -> &mut [u32] {
        let base = y * self.size.w as usize;
        &mut self.buf[base + x0..base + x1]
    }

    /// Fill one horizontal run `[x0, x1)` of row `y` with `color`. Bounds are
    /// the caller's contract.
    #[inline]
    fn set_span(&mut self, y: usize, x0: usize, x1: usize, color: Color) {
        self.span_mut(y, x0, x1).fill(color.packed());
    }

    /// Fill an axis-aligned rectangle with an opaque colour, clipped to bounds.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        if !color.is_visible() {
            return;
        }
        let x0 = rect.origin.x.max(0) as usize;
        let x1 = rect.right().clamp(0, self.size.w as i32) as usize;
        let y0 = rect.origin.y.max(0);
        let y1 = rect.bottom().min(self.size.h as i32);
        if x0 >= x1 {
            return;
        }
        for y in y0..y1 {
            self.set_span(y as usize, x0, x1, color);
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
        let x0 = rect.origin.x.max(0) as usize;
        let x1 = rect.right().clamp(0, self.size.w as i32) as usize;
        let y0 = rect.origin.y.max(0);
        let y1 = rect.bottom().min(self.size.h as i32);
        if x0 >= x1 {
            return;
        }
        for y in y0..y1 {
            for word in self.span_mut(y as usize, x0, x1) {
                *word = blend_word(*word, src, a);
            }
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

/// Clockwise quarter-turn rotation for
/// [`draw_sprite_ex`](Surface::draw_sprite_ex). Only right angles exist —
/// pixel art cannot rotate by an arbitrary angle and stay crisp, so the type
/// makes the constraint unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuarterTurns {
    /// No rotation.
    #[default]
    None,
    /// 90° clockwise.
    Quarter,
    /// 180°.
    Half,
    /// 270° clockwise (90° counter-clockwise).
    ThreeQuarters,
}

/// Options for [`draw_sprite_ex`](Surface::draw_sprite_ex): the source region,
/// mirroring, quarter-turn rotation, and integer magnification. The default is
/// a plain whole-sprite blit — identical to
/// [`draw_sprite`](Surface::draw_sprite). The region is mirrored first, then
/// turned; each output pixel becomes a `scale × scale` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlitOptions {
    /// The source sub-rectangle to draw (`None` = the whole sprite). Clamped
    /// to the sprite's bounds before the transform; a region fully outside
    /// draws nothing.
    pub region: Option<Rect>,
    /// Integer magnification (treated as at least 1) — pixels stay square
    /// blocks.
    pub scale: u32,
    /// Mirror the region horizontally (before rotation).
    pub flip_x: bool,
    /// Mirror the region vertically (before rotation).
    pub flip_y: bool,
    /// Clockwise quarter-turn rotation (after mirroring).
    pub turns: QuarterTurns,
    /// Draw every opaque pixel in this colour instead of its own — a solid
    /// silhouette (a drop shadow behind the real sprite). `None` keeps the
    /// sprite's colours.
    pub tint: Option<Color>,
}

impl Default for BlitOptions {
    fn default() -> Self {
        Self {
            region: None,
            scale: 1,
            flip_x: false,
            flip_y: false,
            turns: QuarterTurns::None,
            tint: None,
        }
    }
}

/// Map one output pixel back to its source cell in region space, given the
/// region's inclusive maxima. One of the eight dihedral maps below.
type MapFn = fn(ox: i32, oy: i32, max_x: i32, max_y: i32) -> (i32, i32);

// The eight dihedral maps. Every map has the same shape: optionally swap the
// output axes (the 90°/270° family), then optionally reflect each source axis
// against its region maximum. `(u, v)` below names the post-swap pair.

/// Identity: `(u, v)` as-is.
fn map_id(ox: i32, oy: i32, _mx: i32, _my: i32) -> (i32, i32) {
    (ox, oy)
}
/// Vertical reflection: `(u, my − v)` — flip-y (or its rotated equivalents).
fn map_neg_y(ox: i32, oy: i32, _mx: i32, my: i32) -> (i32, i32) {
    (ox, my - oy)
}
/// Horizontal reflection: `(mx − u, v)` — flip-x.
fn map_neg_x(ox: i32, oy: i32, mx: i32, _my: i32) -> (i32, i32) {
    (mx - ox, oy)
}
/// Both reflections: `(mx − u, my − v)` — the 180° turn.
fn map_neg_xy(ox: i32, oy: i32, mx: i32, my: i32) -> (i32, i32) {
    (mx - ox, my - oy)
}
/// Axis swap alone: the transpose (a 90° turn combined with one flip).
fn map_swap(ox: i32, oy: i32, _mx: i32, _my: i32) -> (i32, i32) {
    (oy, ox)
}
/// Swap + vertical reflection: the 90° clockwise turn.
fn map_swap_neg_y(ox: i32, oy: i32, _mx: i32, my: i32) -> (i32, i32) {
    (oy, my - ox)
}
/// Swap + horizontal reflection: the 270° clockwise turn.
fn map_swap_neg_x(ox: i32, oy: i32, mx: i32, _my: i32) -> (i32, i32) {
    (mx - oy, ox)
}
/// Swap + both reflections: the anti-transpose.
fn map_swap_neg_xy(ox: i32, oy: i32, mx: i32, my: i32) -> (i32, i32) {
    (mx - oy, my - ox)
}

/// The precomputed transform for one [`draw_sprite_ex`](Surface::draw_sprite_ex)
/// call. `turns × flip_x × flip_y` collapse to the eight elements of the
/// dihedral group, so the per-pixel path is a single indirect call through a
/// map chosen once — no per-pixel branching on the options.
struct SpriteTransform {
    map: MapFn,
    /// Region maxima (`w - 1`, `h - 1`), the constants the maps negate against.
    max_x: i32,
    max_y: i32,
    /// Whether the transform swaps axes (a 90° or 270° turn) — the output size
    /// is transposed, and iteration goes column-outermost to keep source reads
    /// row-major.
    swaps_axes: bool,
    /// The transformed output size in pixels (before scaling).
    out_size: Size,
}

impl SpriteTransform {
    fn new(region: Size, options: &BlitOptions) -> Self {
        // Compose the INVERSE transform (output pixel → source cell) as
        // (swap axes?, negate x?, negate y?). Derivation: a clockwise quarter
        // turn maps source (x, y) to output (h−1−y, x), so its inverse reads
        // x = oy, y = my − ox — an axis swap plus a y-negation; the half turn
        // negates both axes; 270° swaps and negates x. A flip mirrors one
        // source axis after the un-rotation, and mirroring a negated axis
        // un-negates it — so each flip simply toggles its axis's negation.
        let (swaps_axes, mut neg_x, mut neg_y) = match options.turns {
            QuarterTurns::None => (false, false, false),
            QuarterTurns::Quarter => (true, false, true),
            QuarterTurns::Half => (false, true, true),
            QuarterTurns::ThreeQuarters => (true, true, false),
        };
        neg_x ^= options.flip_x;
        neg_y ^= options.flip_y;
        // The exhaustive match keeps map selection compiler-checked (no index
        // packing to get subtly wrong).
        let map: MapFn = match (swaps_axes, neg_x, neg_y) {
            (false, false, false) => map_id,
            (false, false, true) => map_neg_y,
            (false, true, false) => map_neg_x,
            (false, true, true) => map_neg_xy,
            (true, false, false) => map_swap,
            (true, false, true) => map_swap_neg_y,
            (true, true, false) => map_swap_neg_x,
            (true, true, true) => map_swap_neg_xy,
        };
        Self {
            map,
            max_x: region.w as i32 - 1,
            max_y: region.h as i32 - 1,
            swaps_axes,
            out_size: if swaps_axes {
                Size::new(region.h, region.w)
            } else {
                region
            },
        }
    }

    /// The source cell (in region space) behind output pixel `(ox, oy)`.
    #[inline]
    fn source(&self, ox: i32, oy: i32) -> (i32, i32) {
        (self.map)(ox, oy, self.max_x, self.max_y)
    }
}

/// The caller's region clamped to the sprite's bounds (`None` argument = the
/// whole sprite). `None` result = nothing to draw.
#[inline]
fn clamped_region(sprite: Size, region: Option<Rect>) -> Option<Rect> {
    let Some(r) = region else {
        return Some(Rect::from_size(sprite));
    };
    let x0 = i64::from(r.origin.x).clamp(0, i64::from(sprite.w));
    let y0 = i64::from(r.origin.y).clamp(0, i64::from(sprite.h));
    let x1 = (i64::from(r.origin.x) + i64::from(r.size.w)).clamp(0, i64::from(sprite.w));
    let y1 = (i64::from(r.origin.y) + i64::from(r.size.h)).clamp(0, i64::from(sprite.h));
    (x1 > x0 && y1 > y0).then(|| {
        Rect::new(
            Point::new(x0 as i32, y0 as i32),
            Size::new((x1 - x0) as u32, (y1 - y0) as u32),
        )
    })
}

/// The output-pixel indices in `0..count` whose `scale`-wide blocks — block
/// `i` spans `[offset + i·scale, offset + (i+1)·scale)` — intersect
/// `[0, limit)`. Destination clipping, computed once per axis.
#[inline]
fn visible_blocks(offset: i64, scale: i64, count: i64, limit: i64) -> std::ops::Range<i64> {
    let first = if offset < 0 { (-offset) / scale } else { 0 };
    let span = limit - offset;
    let end = if span <= 0 {
        0
    } else {
        count.min((span + scale - 1) / scale)
    };
    first.min(end)..end
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

    // ---- draw_sprite_ex ----

    /// A 3×2 sprite of six distinct colours:
    /// ```text
    ///   A B C
    ///   D E F
    /// ```
    fn abcdef() -> (Sprite, [Color; 6]) {
        let colors = [
            Color::rgb(10, 0, 0),
            Color::rgb(20, 0, 0),
            Color::rgb(30, 0, 0),
            Color::rgb(40, 0, 0),
            Color::rgb(50, 0, 0),
            Color::rgb(60, 0, 0),
        ];
        let mut spr = Sprite::new(Size::new(3, 2));
        for (i, &c) in colors.iter().enumerate() {
            spr.set(Point::new((i % 3) as i32, (i / 3) as i32), c);
        }
        (spr, colors)
    }

    /// Assert the surface holds `expected` (a row-major grid `cols` wide) at
    /// the origin.
    fn assert_grid(s: &Surface, cols: u32, expected: &[Color]) {
        for (i, &c) in expected.iter().enumerate() {
            let (x, y) = (i as u32 % cols, i as u32 / cols);
            assert_eq!(word_at(s, x, y), c.packed(), "at ({x},{y})");
        }
    }

    #[test]
    fn ex_default_options_match_draw_sprite() {
        let (spr, _) = abcdef();
        let bg = Color::rgb(0, 0, 0);
        let mut plain = Surface::new(Size::new(4, 3), bg);
        let mut ex = Surface::new(Size::new(4, 3), bg);
        plain.draw_sprite(&spr, Point::new(1, 1));
        ex.draw_sprite_ex(&spr, Point::new(1, 1), BlitOptions::default());
        assert_eq!(plain.as_slice(), ex.as_slice());
    }

    #[test]
    fn ex_region_draws_only_the_subrect() {
        let (spr, c) = abcdef();
        let mut s = Surface::new(Size::new(2, 2), Color::rgb(0, 0, 0));
        s.draw_sprite_ex(
            &spr,
            Point::ORIGIN,
            BlitOptions {
                region: Some(Rect::new(Point::new(1, 0), Size::new(2, 2))),
                ..BlitOptions::default()
            },
        );
        // B C / E F.
        assert_grid(&s, 2, &[c[1], c[2], c[4], c[5]]);
    }

    #[test]
    fn ex_scale_expands_each_pixel_into_blocks() {
        let (spr, c) = abcdef();
        let mut s = Surface::new(Size::new(2, 2), Color::rgb(0, 0, 0));
        s.draw_sprite_ex(
            &spr,
            Point::ORIGIN,
            BlitOptions {
                region: Some(Rect::new(Point::ORIGIN, Size::new(1, 1))),
                scale: 2,
                ..BlitOptions::default()
            },
        );
        assert_grid(&s, 2, &[c[0], c[0], c[0], c[0]]);
    }

    #[test]
    fn ex_flip_x_mirrors_horizontally() {
        let (spr, c) = abcdef();
        let mut s = Surface::new(Size::new(3, 2), Color::rgb(0, 0, 0));
        s.draw_sprite_ex(
            &spr,
            Point::ORIGIN,
            BlitOptions {
                flip_x: true,
                ..BlitOptions::default()
            },
        );
        // C B A / F E D.
        assert_grid(&s, 3, &[c[2], c[1], c[0], c[5], c[4], c[3]]);
    }

    #[test]
    fn ex_flip_y_mirrors_vertically() {
        let (spr, c) = abcdef();
        let mut s = Surface::new(Size::new(3, 2), Color::rgb(0, 0, 0));
        s.draw_sprite_ex(
            &spr,
            Point::ORIGIN,
            BlitOptions {
                flip_y: true,
                ..BlitOptions::default()
            },
        );
        // D E F / A B C.
        assert_grid(&s, 3, &[c[3], c[4], c[5], c[0], c[1], c[2]]);
    }

    #[test]
    fn ex_quarter_turn_rotates_clockwise() {
        let (spr, c) = abcdef();
        let mut s = Surface::new(Size::new(2, 3), Color::rgb(0, 0, 0));
        s.draw_sprite_ex(
            &spr,
            Point::ORIGIN,
            BlitOptions {
                turns: QuarterTurns::Quarter,
                ..BlitOptions::default()
            },
        );
        // D A / E B / F C.
        assert_grid(&s, 2, &[c[3], c[0], c[4], c[1], c[5], c[2]]);
    }

    #[test]
    fn ex_half_turn_rotates_180() {
        let (spr, c) = abcdef();
        let mut s = Surface::new(Size::new(3, 2), Color::rgb(0, 0, 0));
        s.draw_sprite_ex(
            &spr,
            Point::ORIGIN,
            BlitOptions {
                turns: QuarterTurns::Half,
                ..BlitOptions::default()
            },
        );
        // F E D / C B A.
        assert_grid(&s, 3, &[c[5], c[4], c[3], c[2], c[1], c[0]]);
    }

    #[test]
    fn ex_three_quarter_turn_rotates_counter_clockwise() {
        let (spr, c) = abcdef();
        let mut s = Surface::new(Size::new(2, 3), Color::rgb(0, 0, 0));
        s.draw_sprite_ex(
            &spr,
            Point::ORIGIN,
            BlitOptions {
                turns: QuarterTurns::ThreeQuarters,
                ..BlitOptions::default()
            },
        );
        // C F / B E / A D.
        assert_grid(&s, 2, &[c[2], c[5], c[1], c[4], c[0], c[3]]);
    }

    #[test]
    fn ex_mirror_applies_before_the_turn() {
        let (spr, c) = abcdef();
        let mut s = Surface::new(Size::new(2, 3), Color::rgb(0, 0, 0));
        s.draw_sprite_ex(
            &spr,
            Point::ORIGIN,
            BlitOptions {
                flip_x: true,
                turns: QuarterTurns::Quarter,
                ..BlitOptions::default()
            },
        );
        // Mirror first (C B A / F E D), then 90° CW: F C / E B / D A.
        // (Turning first and mirroring after would read A D / B E / C F.)
        assert_grid(&s, 2, &[c[5], c[2], c[4], c[1], c[3], c[0]]);
    }

    #[test]
    fn ex_transparent_pixels_are_skipped() {
        let mut spr = Sprite::new(Size::new(2, 1));
        let red = Color::rgb(255, 0, 0);
        spr.set(Point::ORIGIN, red); // (1,0) stays transparent
        let bg = Color::rgb(1, 2, 3);
        let mut s = Surface::new(Size::new(2, 1), bg);
        s.draw_sprite_ex(
            &spr,
            Point::ORIGIN,
            BlitOptions {
                flip_x: true,
                ..BlitOptions::default()
            },
        );
        // Mirrored: the transparent cell lands at x=0, the red one at x=1.
        assert_eq!(word_at(&s, 0, 0), bg.packed());
        assert_eq!(word_at(&s, 1, 0), red.packed());
    }

    #[test]
    fn ex_clips_at_the_edges_without_panicking() {
        let (spr, c) = abcdef();
        let mut s = Surface::new(Size::new(2, 2), Color::rgb(0, 0, 0));
        s.draw_sprite_ex(&spr, Point::new(-1, -1), BlitOptions::default());
        // Only the sprite's inner cells land: (0,0) shows E — source (1,1).
        assert_eq!(word_at(&s, 0, 0), c[4].packed());
        assert_eq!(word_at(&s, 1, 0), c[5].packed());
    }

    #[test]
    fn ex_a_region_past_the_sprite_is_clamped() {
        let (spr, c) = abcdef();
        let bg = Color::rgb(0, 0, 0);
        let mut s = Surface::new(Size::new(4, 2), bg);
        s.draw_sprite_ex(
            &spr,
            Point::ORIGIN,
            BlitOptions {
                region: Some(Rect::new(Point::new(2, 0), Size::new(2, 1))),
                ..BlitOptions::default()
            },
        );
        // The 2-wide region clamps to the sheet's last real column.
        assert_eq!(word_at(&s, 0, 0), c[2].packed()); // C
        assert_eq!(word_at(&s, 1, 0), bg.packed()); // clamped away: nothing
    }

    #[test]
    fn ex_a_region_fully_outside_draws_nothing() {
        let (spr, _) = abcdef();
        let bg = Color::rgb(0, 0, 0);
        let mut s = Surface::new(Size::new(2, 2), bg);
        s.draw_sprite_ex(
            &spr,
            Point::ORIGIN,
            BlitOptions {
                region: Some(Rect::new(Point::new(10, 10), Size::new(2, 2))),
                ..BlitOptions::default()
            },
        );
        assert_eq!(word_at(&s, 0, 0), bg.packed());
    }

    #[test]
    fn ex_zero_scale_draws_nothing() {
        let (spr, _) = abcdef();
        let bg = Color::rgb(0, 0, 0);
        let mut s = Surface::new(Size::new(3, 2), bg);
        s.draw_sprite_ex(
            &spr,
            Point::ORIGIN,
            BlitOptions {
                scale: 0,
                ..BlitOptions::default()
            },
        );
        // An explicit no-op, not a silent coercion to 1.
        assert_eq!(word_at(&s, 0, 0), bg.packed());
        assert_eq!(word_at(&s, 2, 1), bg.packed());
    }

    #[test]
    fn ex_scaled_blocks_clip_partially_at_the_edges() {
        // A 1×1 region at scale 3, placed at (-1,-1): only the block's inner
        // 2×2 corner is on-screen — the span clipping must trim it exactly.
        let (spr, c) = abcdef();
        let bg = Color::rgb(0, 0, 0);
        let mut s = Surface::new(Size::new(3, 3), bg);
        s.draw_sprite_ex(
            &spr,
            Point::new(-1, -1),
            BlitOptions {
                region: Some(Rect::new(Point::ORIGIN, Size::new(1, 1))),
                scale: 3,
                ..BlitOptions::default()
            },
        );
        assert_grid(&s, 3, &[c[0], c[0], bg, c[0], c[0], bg, bg, bg, bg]);
    }

    #[test]
    fn ex_tint_draws_a_silhouette_through_the_transform() {
        let (spr, _) = abcdef();
        let bg = Color::rgb(0, 0, 0);
        let yellow = Color::rgb(255, 255, 0);
        let mut s = Surface::new(Size::new(3, 2), bg);
        s.draw_sprite_ex(
            &spr,
            Point::ORIGIN,
            BlitOptions {
                flip_x: true,
                tint: Some(yellow),
                ..BlitOptions::default()
            },
        );
        // Every opaque pixel lands in the tint, whatever its own colour.
        assert_grid(&s, 3, &[yellow; 6]);
    }

    #[test]
    fn transform_swaps_output_size_only_on_quarter_turns() {
        let region = Size::new(3, 2);
        for (turns, expected) in [
            (QuarterTurns::None, Size::new(3, 2)),
            (QuarterTurns::Quarter, Size::new(2, 3)),
            (QuarterTurns::Half, Size::new(3, 2)),
            (QuarterTurns::ThreeQuarters, Size::new(2, 3)),
        ] {
            let options = BlitOptions {
                turns,
                ..BlitOptions::default()
            };
            let t = SpriteTransform::new(region, &options);
            assert_eq!(t.out_size, expected, "{turns:?}");
            assert_eq!(
                t.swaps_axes,
                matches!(turns, QuarterTurns::Quarter | QuarterTurns::ThreeQuarters)
            );
        }
    }

    #[test]
    fn visible_blocks_clips_both_ends() {
        // Blocks of 2 starting at -3: block 0 spans [-3,-1) (out), block 1
        // spans [-1,1) (partly in) … block 4 spans [5,7) but the limit is 6.
        assert_eq!(visible_blocks(-3, 2, 10, 6), 1..5);
        // Fully left / fully right of the surface: empty.
        assert_eq!(visible_blocks(-20, 2, 5, 6), 5..5);
        assert_eq!(visible_blocks(9, 2, 5, 6), 0..0);
    }
}

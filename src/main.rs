//! ratgames — an 8-bit *style* marquee banner.
//!
//! "8-bit" is an aesthetic, not a hardware limit, and the retro constraint is a
//! real pixel resolution (NES was 256x240; we use a clean 256x256 square), not
//! a VT100 character grid. The scene is composed in a fixed **virtual screen**
//! (`VW` x `VH`) at its native pixel resolution, then upscaled with a single
//! clean integer nearest-neighbour blit (letterboxed to preserve aspect) to
//! fill whatever the physical viewport is. Owning the framebuffer also
//! sidesteps Terminal.app's Quartz glyph anti-aliasing/repositioning — there
//! are no cells to tear.
//!
//! Two integer scales, both crisp:
//!   * `GLYPH_SCALE`  font-pixel -> virtual-pixel   (title drawn as a sprite)
//!   * `fit_scale`    virtual-pixel -> physical-pixel (viewport fit, per-frame)

use font8x8::legacy::BASIC_LEGACY;
use minifb::{Key, Window, WindowOptions};

// ---- Virtual screen — a real retro pixel resolution ------------------------

const VW: usize = 256;
const VH: usize = 256;

// ---- Physical viewport -----------------------------------------------------

/// Initial window size (resizable). 3x of the 256 screen fills it exactly;
/// maximise to 1920x1080 and it re-fits to 4x, pillarboxed.
const WIN_W: usize = 768;
const WIN_H: usize = 768;

// ---- Layout / effect tunables ----------------------------------------------

const GLYPH_W: usize = 8;
const GLYPH_H: usize = 8;

/// Art-pixels per font-pixel: how large the title sprite is drawn on the
/// 256x256 screen. 6 -> 48px-tall letters (~5 visible; the rest scrolls).
const GLYPH_SCALE: usize = 6;

/// Blank font-pixel columns between glyphs.
const TRACKING: usize = 1;
/// Trailing blank columns so the marquee has a gap before it wraps.
const GAP_COLS: usize = 14;
/// Depth of the down-right yellow extrusion, in font pixels.
const SHADOW_DEPTH: usize = 3;
/// Outline padding above the glyph row inside the grid.
const PAD_TOP: usize = 1;
/// Grid height carries the glyph plus room for its outline + shadow halo, so
/// the bottom row's shadow is not clipped.
const GRID_H: usize = PAD_TOP + GLYPH_H + SHADOW_DEPTH + 1;

/// Marquee speed, in virtual (256-res) pixels per frame. Integer on purpose —
/// keeps the upscale a pure nearest-neighbour, no interpolation blur.
const SCROLL_VPX_PER_FRAME: usize = 2;

// ---- Palette (0x00RRGGBB; minifb ignores the top byte) ---------------------

const BG: u32 = 0x0018_1830; // dark slate — backdrop of the virtual screen
const FILL: u32 = 0x0039_D353; // bright NES green
const OUTLINE: u32 = 0x0000_0000; // black border
const SHADOW: u32 = 0x00F2_C40C; // golden-yellow 3D shadow
const LETTERBOX: u32 = 0x0000_0000; // bars around the fitted screen

/// The effect layer an art-pixel resolves to. Closed state machine; precedence
/// is fixed: `Fill` > `Outline` > `Shadow` > `Background`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Background,
    Shadow,
    Outline,
    Fill,
}

impl Layer {
    const fn color(self) -> u32 {
        match self {
            Layer::Background => BG,
            Layer::Shadow => SHADOW,
            Layer::Outline => OUTLINE,
            Layer::Fill => FILL,
        }
    }
}

/// The banner rasterised once at font resolution: a `cols`-wide, `GRID_H`-tall
/// grid of baked colours — a sprite strip the virtual screen samples (scaled by
/// `GLYPH_SCALE`) with a horizontal scroll offset.
#[derive(Debug)]
struct Banner {
    cols: usize,
    /// `cols * GRID_H`, row-major; each entry is a final `0x00RRGGBB` colour.
    grid: Vec<u32>,
}

impl Banner {
    /// Rasterise `text`: green fill, black outline, down-right extruded yellow
    /// shadow. Unknown / non-ASCII chars render as blank cells rather than
    /// failing.
    fn render(text: &str) -> Self {
        // 1. Lay glyphs into a boolean "on" mask (glyph rows offset by PAD_TOP).
        let glyphs: Vec<[u8; 8]> = text.chars().map(glyph).collect();
        let cols = glyphs.len() * (GLYPH_W + TRACKING) + GAP_COLS;

        let mut on = vec![false; cols * GRID_H];
        for (i, g) in glyphs.iter().enumerate() {
            let x0 = i * (GLYPH_W + TRACKING);
            for (row, bits) in g.iter().enumerate() {
                for col in 0..GLYPH_W {
                    // font8x8: bit 0 (LSB) is the leftmost column.
                    if (bits >> col) & 1 == 1 {
                        on[(row + PAD_TOP) * cols + (x0 + col)] = true;
                    }
                }
            }
        }

        // Sampler with horizontal wrap so the marquee seam is seamless; the
        // vertical axis does not wrap (out-of-band rows read as empty).
        let at = |x: isize, y: isize| -> bool {
            if y < 0 || y >= GRID_H as isize {
                return false;
            }
            let xm = x.rem_euclid(cols as isize) as usize;
            on[y as usize * cols + xm]
        };

        // 2. Classify every cell by precedence and bake its colour.
        let mut grid = vec![BG; cols * GRID_H];
        for y in 0..GRID_H as isize {
            for x in 0..cols as isize {
                let layer = if at(x, y) {
                    Layer::Fill
                } else if has_fill_neighbor(&at, x, y) {
                    Layer::Outline
                } else if is_shadow(&at, x, y) {
                    Layer::Shadow
                } else {
                    Layer::Background
                };
                grid[y as usize * cols + x as usize] = layer.color();
            }
        }

        Self { cols, grid }
    }

    /// Full banner width in virtual (256-res) pixels — one marquee period.
    const fn width_vpx(&self) -> usize {
        self.cols * GLYPH_SCALE
    }
}

/// Largest integer scale at which the virtual screen fits in `win_w x win_h`.
/// This is the "scales cleanly" contract: integer-only, so every art pixel
/// stays a crisp square; the remainder is letterboxed, never fractionally
/// stretched.
const fn fit_scale(win_w: usize, win_h: usize) -> usize {
    let s = if win_w / VW < win_h / VH {
        win_w / VW
    } else {
        win_h / VH
    };
    if s == 0 {
        1
    } else {
        s
    }
}

/// Compose the banner into `canvas` (`VW * VH`) at native resolution: the title
/// sprite scaled by `GLYPH_SCALE`, vertically centred, scrolled left by
/// `scroll_vpx` virtual pixels (wrapping seamlessly).
fn compose(canvas: &mut [u32], banner: &Banner, scroll_vpx: usize) {
    canvas.fill(BG);
    let strip_h = GRID_H * GLYPH_SCALE;
    let band_top = VH.saturating_sub(strip_h) / 2;
    let period = banner.width_vpx();

    for ry in 0..strip_h {
        let vy = band_top + ry;
        if vy >= VH {
            break;
        }
        let src_row = (ry / GLYPH_SCALE) * banner.cols;
        let dst = vy * VW;
        for vx in 0..VW {
            let col = ((vx + scroll_vpx) % period) / GLYPH_SCALE;
            canvas[dst + vx] = banner.grid[src_row + col];
        }
    }
}

/// Present `canvas` into the window buffer: single integer nearest-neighbour
/// upscale, centred, with letterbox bars filling the remainder.
fn present(canvas: &[u32], win: &mut [u32], win_w: usize, win_h: usize) {
    let scale = fit_scale(win_w, win_h);
    let dst_w = VW * scale;
    let dst_h = VH * scale;
    let off_x = win_w.saturating_sub(dst_w) / 2;
    let off_y = win_h.saturating_sub(dst_h) / 2;

    win.fill(LETTERBOX);
    for vy in 0..VH {
        let src = vy * VW;
        for sy in 0..scale {
            let py = off_y + vy * scale + sy;
            if py >= win_h {
                break;
            }
            let row = py * win_w;
            for vx in 0..VW {
                let c = canvas[src + vx];
                let base = row + off_x + vx * scale;
                for sx in 0..scale {
                    if off_x + vx * scale + sx >= win_w {
                        break;
                    }
                    win[base + sx] = c;
                }
            }
        }
    }
}

/// The 8x8 bitmap for `c`, or a blank cell for anything outside ASCII.
fn glyph(c: char) -> [u8; 8] {
    let i = c as usize;
    if i < BASIC_LEGACY.len() {
        BASIC_LEGACY[i]
    } else {
        [0; 8]
    }
}

/// True if any 8-neighbour is a fill cell — i.e. this cell is on the black rim.
fn has_fill_neighbor(at: &impl Fn(isize, isize) -> bool, x: isize, y: isize) -> bool {
    for dy in -1..=1 {
        for dx in -1..=1 {
            if (dx, dy) != (0, 0) && at(x + dx, y + dy) {
                return true;
            }
        }
    }
    false
}

/// True if this cell lies on the down-right extrusion of a fill cell.
fn is_shadow(at: &impl Fn(isize, isize) -> bool, x: isize, y: isize) -> bool {
    (1..=SHADOW_DEPTH as isize).any(|k| at(x - k, y - k))
}

fn main() -> Result<(), minifb::Error> {
    let text = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "YOU WIN!!".to_string());
    let banner = Banner::render(&text);
    let period = banner.width_vpx();

    let mut window = Window::new(
        "ratgames",
        WIN_W,
        WIN_H,
        WindowOptions {
            resize: true,
            ..WindowOptions::default()
        },
    )?;
    window.set_target_fps(60);

    let mut canvas = vec![BG; VW * VH];
    let (mut cur_w, mut cur_h) = window.get_size();
    let mut win_buf = vec![LETTERBOX; cur_w * cur_h];
    let mut scroll_vpx = 0usize;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let (w, h) = window.get_size();
        if (w, h) != (cur_w, cur_h) {
            cur_w = w;
            cur_h = h;
            win_buf.resize(w * h, LETTERBOX);
        }

        compose(&mut canvas, &banner, scroll_vpx);
        present(&canvas, &mut win_buf, cur_w, cur_h);
        window.update_with_buffer(&win_buf, cur_w, cur_h)?;

        scroll_vpx = (scroll_vpx + SCROLL_VPX_PER_FRAME) % period;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_has_all_three_effect_layers() {
        let b = Banner::render("A");
        let has = |c: u32| b.grid.iter().any(|&p| p == c);
        assert!(has(FILL), "expected green fill cells");
        assert!(has(OUTLINE), "expected black outline cells");
        assert!(has(SHADOW), "expected yellow shadow cells");
    }

    #[test]
    fn unknown_glyph_is_blank_not_panic() {
        let b = Banner::render("é");
        assert!(b.grid.iter().all(|&p| p == BG));
    }

    #[test]
    fn fit_scale_is_integer_and_letterboxes() {
        // 1080p over a 256x256 screen: height-bound at 4x, pillarboxed width.
        assert_eq!(fit_scale(1920, 1080), 4);
        // Default 768x768 window fills at exactly 3x.
        assert_eq!(fit_scale(768, 768), 3);
        // Never collapses to 0, even for an absurdly small window.
        assert_eq!(fit_scale(1, 1), 1);
    }
}

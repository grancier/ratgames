# ratgames

An 8-bit *style* marquee banner, rendered to a native framebuffer window.

A phrase like `YOU WIN!!` scrolls across a retro 256×256 virtual screen as
oversized pixel letters — bold green fill, black outline, golden-yellow 3D
shadow — and the whole screen is upscaled cleanly to fill the window.

## Why a window and not the terminal

Terminal.app's Quartz text renderer anti-aliases and repositions glyphs
(including the half/quadrant blocks that widgets like `tui-big-text` rely on)
at cell boundaries, which shows up as hairline gaps / "tearing" between rows.
Owning the framebuffer sidesteps the cell grid entirely: every pixel is ours,
so there is nothing to anti-alias or reposition.

## How it looks 8-bit without being low-resolution

"8-bit" is an aesthetic, not a device limit. The scene is composed on a fixed
**virtual screen** at a real retro pixel resolution (256×256, in the spirit of
the NES's 256×240), then scaled to the physical window in a **single clean
integer nearest-neighbour blit**, letterboxed to preserve aspect. Two integer
scales, both crisp:

| Scale | Maps | Set by |
|---|---|---|
| `GLYPH_SCALE` | font-pixel → virtual-pixel | the title sprite size on the screen |
| `fit_scale` | virtual-pixel → physical-pixel | derived from the window each frame |

Because both are integers, art pixels stay crisp squares; non-integer window
sizes letterbox rather than fractionally stretch (which would blur). The
marquee scrolls in whole virtual pixels for the same reason — sub-pixel motion
would force the upscale to interpolate and reintroduce blur.

## Pipeline

```
Banner::render(text)   font-resolution sprite (green/black/yellow baked in)
        │
        ▼  compose()   draw scaled by GLYPH_SCALE into the 256×256 screen;
        │              the marquee scroll offset lives here
        ▼  present()   one integer nearest-neighbour upscale + letterbox
        │              into the window buffer
        ▼
   minifb window
```

## Run

```sh
cargo run --release                  # "YOU WIN!!"
cargo run --release -- "GAME OVER"   # any ASCII text
```

`Esc` or closing the window quits. The window is resizable; the banner re-fits
to the new size every frame.

## Tuning

All knobs are `const`s at the top of [`src/main.rs`](src/main.rs):

- `VW` / `VH` — virtual screen resolution (use `256×144` for a 16:9 fill)
- `GLYPH_SCALE` — letter size on the screen (~5 chars visible at `6`)
- `SCROLL_VPX_PER_FRAME` — marquee speed
- `SHADOW_DEPTH` — depth of the yellow extrusion
- `BG` / `FILL` / `OUTLINE` / `SHADOW` — palette

## Dependencies

- [`minifb`](https://crates.io/crates/minifb) — window + raw `u32` framebuffer
- [`font8x8`](https://crates.io/crates/font8x8) — 8×8 bitmap glyphs

# ratgames

A small Rust toolkit for rendering an **8-bit-style** scene to a native
framebuffer window — oversized pixel-art banners, a scrolling marquee, an
anti-aliased input field, and a tiny math quiz built on top. It is a
*presentation toolkit* (a library crate) with a thin demo binary and a worked
`math_game` example, not a game engine.

## Why a window and not the terminal

Terminal.app's Quartz text renderer anti-aliases and repositions glyphs
(including the half/quadrant blocks that widgets like `tui-big-text` rely on) at
cell boundaries, which shows up as hairline gaps / "tearing" between rows. Owning
the framebuffer (via [`minifb`](https://crates.io/crates/minifb), a `Vec<u32>`)
sidesteps the cell grid entirely: every pixel is ours, so there is nothing to
anti-alias or reposition — and input comes from the window, not a TTY.

## "8-bit" is an aesthetic, not a resolution limit

The scene is composed on a fixed low-resolution **virtual screen** (256×256, in
the spirit of the NES's 256×240), then presented to the physical window with a
**single integer nearest-neighbour upscale + letterbox**. Integer-only scaling
keeps art pixels crisp squares; non-integer window sizes letterbox rather than
fractionally stretch (which would blur), and the marquee scrolls in whole virtual
pixels so the upscale never interpolates.

Below the size where the virtual screen fits, the upscale holds a **crisp-clip
floor** (`ScreenConfig::min_scale`, default 1×) and lets the screen clip rather
than shrink fractionally — a named policy in `Presentation::fit_scale`.

## Two rendering pipelines

The architecture pivots on keeping two coordinate spaces distinct:

- **`PixelLayer`** draws into the 256×256 virtual screen — the crisp 8-bit world,
  later integer-upscaled. Sprites, the `Marquee`, the `Placard`.
- **`OverlayLayer`** draws into the window in **device pixels, after** the
  upscale — for content that must never be pixel-scaled, i.e. the anti-aliased
  input font.

`Presentation` composites a frame — clear → pixel layers → upscale + letterbox →
overlay layers — and depends only on `&[&dyn PixelLayer]` / `&[&dyn OverlayLayer]`,
so new content plugs in without touching the compositor.

Key modules: `surface` (the blittable `Vec<u32>` buffer), `sprite` / `color` /
`geometry` (primitives), `text` (pixel-art `BigText` → `Sprite`), `marquee` /
`placard` (pixel layers), `font` (fontdue + fontdb AA rasterisation), `input`
(the AA input overlay), `quiz` (the math-quiz state machine), and `config`.

## Configuration

Every dimension, colour, size, timing, and font setting lives in the `Config`
tree in [`src/config.rs`](src/config.rs) — the in-code "header" of defaults.
Nothing downstream hardcodes a magic literal; a re-theme or re-tune touches only
`Config`.

The tree is `serde`-serialisable: `Config::load(path)` reads a TOML file,
falling back to the default for any field it omits, then validates ranges
(non-zero dimensions, a `(0, 1]` panel fraction). Colours are `#RRGGBB` /
`#AARRGGBB` strings and default from a named `Theme`, so a re-skin is a `[theme]`
table. Coupling is deliberately light: a theme in the file restyles defaults,
but a colour a config sets explicitly still wins.

## Run

```sh
cargo run                        # the marquee demo: "YOU WIN!!" + input field
cargo run -- "GAME OVER"         # custom banner text
cargo run --example math_game    # the math quiz: answer 6+6, retry loop, win
cargo test                       # unit + integration tests
cargo test -- --ignored          # + tests that need a system font (Menlo)
cargo clippy --all-targets       # lints
```

Type into the input field; **Backspace** edits, **Enter** submits, **Esc** (or
closing the window) quits. The window is resizable and the virtual screen re-fits
each frame.

## Dependencies

- [`minifb`](https://crates.io/crates/minifb) — window + raw `u32` framebuffer
- [`font8x8`](https://crates.io/crates/font8x8) — 8×8 bitmap glyphs (pixel-art text)
- [`fontdue`](https://crates.io/crates/fontdue) — anti-aliased glyph rasterisation
- [`fontdb`](https://crates.io/crates/fontdb) — system-font resolution by family
- [`thiserror`](https://crates.io/crates/thiserror) — typed library errors
- [`anyhow`](https://crates.io/crates/anyhow) — error handling in the binary
- [`serde`](https://crates.io/crates/serde) + [`toml`](https://crates.io/crates/toml) — config (de)serialization

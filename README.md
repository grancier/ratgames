# ratgames

A small Rust workspace for building retro framebuffer games without turning the
renderer into a terminal UI or a full game engine.

```text
ratgames        reusable presentation/session/host primitives
mathgame-core   dependency-free math problem generation and answer evaluation
mathgame-app    first playable arcade-style math game
```

The root `ratgames` crate is a *presentation toolkit*: it renders an
8-bit-style virtual scene to a native framebuffer, provides reusable UI/session
primitives, and exposes an optional native host backend. Product rules and
composition live outside the toolkit.

## Why a window and not the terminal

Terminal.app's Quartz text renderer anti-aliases and repositions glyphs
(including the half/quadrant blocks that widgets like `tui-big-text` rely on) at
cell boundaries, which shows up as hairline gaps / "tearing" between rows. Owning
the framebuffer (via [`minifb`](https://crates.io/crates/minifb), a `Vec<u32>`)
sidesteps the cell grid entirely: every pixel is ours, so there is nothing to
anti-alias or reposition — and input comes from the window, not a TTY.

## "8-bit" is an aesthetic, not a resolution limit

The scene is composed on a low-resolution **virtual screen** chosen by device
class (desktop defaults to 320×180, with mobile/tablet overrides), then presented
to the physical window with a **single integer nearest-neighbour upscale +
letterbox**. Integer-only scaling keeps art pixels crisp squares; non-integer
window sizes letterbox rather than fractionally stretch (which would blur), and
the marquee scrolls in whole virtual pixels so the upscale never interpolates.

Below the size where the virtual screen fits, the upscale holds a **crisp-clip
floor** (`ScreenConfig::min_scale`, default 1×) and lets the screen clip rather
than shrink fractionally — a named policy in `Presentation::fit_scale`.

Text detail is independent of `scale`: raising `scale` magnifies a glyph source,
it does not add resolution. The chunky default source is `font8x8` (8×8 bitmaps);
for higher-resolution lettering a banner's `glyph_source` can be a TTF rasterised
at a chosen cell size (`GlyphSourceConfig::Raster { cell_px, font }`), which gets
the same retro outline / shadow / integer-upscale treatment over genuinely more
detail. Outline thickness (`outline_px`) is a config knob too.

## Two rendering pipelines

The architecture pivots on keeping two coordinate spaces distinct:

- **`PixelLayer`** draws into the virtual screen — the crisp 8-bit world, later
  integer-upscaled. Sprites, the `Marquee`, the `Placard`.
- **`OverlayLayer`** draws into the window in **device pixels, after** the
  upscale — for content that needs real framebuffer pixels: anti-aliased text
  and pixel-art overlays such as `ShadowBanner`, whose CSS-like shadow offset is
  measured after the upscale.

`Presentation` composites a frame — clear → pixel layers → upscale + letterbox →
overlay layers — and depends only on `&[&dyn PixelLayer]` / `&[&dyn OverlayLayer]`,
so new content plugs in without touching the compositor.

Key modules: `surface` (the blittable `Vec<u32>` buffer), `sprite` / `color` /
`geometry` (primitives), `text` (pixel-art `BigText` -> `Sprite`), `marquee` /
`placard` (pixel layers), `font` (fontdue + fontdb AA rasterisation), `input`
(the AA input overlay), `session` (arcade/session primitives), `ui` (reusable
menus, panels, labels, `ShadowBanner`, and layout helpers), `host::minifb`
(optional native window backend), and `config`.

## Configuration

Engine-level dimensions, colours, sizes, timing, and font settings live in the
`Config` tree in [`src/config/`](src/config/mod.rs) — the in-code "header" of
toolkit defaults. Re-theme and renderer tuning belongs there, not scattered
through drawing code.

The tree is `serde`-serialisable: `Config::load(path)` reads a TOML or JSON file
(chosen by extension), falling back to the default for any field it omits, then
validates ranges (non-zero dimensions, a `(0, 1]` panel fraction). Colours are
`#RRGGBB` / `#AARRGGBB` strings and default from a named `Theme`, so a re-skin is
a `[theme]` table. Coupling is deliberately light: a theme in the file restyles
defaults, but a colour a config sets explicitly still wins.

Runtime config comes only from files or Rust — never environment variables (those
are reserved for build-time settings). The examples take an explicit
`--config <path>` (`.toml` / `.json`) or fall back to typed defaults. Validation
rejects a zero `text_scale`/`scale`, an out-of-range raster `cell_px`, and
banners large enough to read as a mistyped size rather than a design.
[`examples/marquee.toml`](examples/marquee.toml) /
[`.json`](examples/marquee.json) are ready-to-run samples that swap the chunky
8×8 marquee for a Menlo TTF rasterised at `cell_px = 32`; `math_game` renders its
banners through the raster source out of the box.

`mathgame-app` wraps `ratgames::Config` in its own `AppConfig`, loaded from a
bundled JSON default or a `--config <path>` TOML/JSON override. The app owns
product-specific presentation settings and high-score persistence; `mathgame-core`
stays pure domain logic with no renderer, storage, or config dependency.

## Run

```sh
cargo run -p mathgame-app                                   # playable mathgame app

cargo run --example marquee --features minifb               # scrolling "YOU WIN!!" + input field
cargo run --example marquee --features minifb -- "GAME OVER" # custom banner text
cargo run --example math_game --features minifb             # legacy quiz example
cargo run --example marquee --features minifb -- --config examples/marquee.toml "HELLO"
cargo run --example marquee --features minifb -- --config examples/marquee.json "HELLO"

cargo fmt --check
cargo test --workspace
cargo test --workspace -- --ignored                         # system-font tests
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p ratgames                                     # pure toolkit, no minifb
cargo test -p ratgames --features minifb                    # optional native host
cargo clippy -p ratgames --features minifb --all-targets -- -D warnings
```

Type into the input field; **Backspace** edits, **Enter** submits, **Esc** (or
closing the window) quits. The window is resizable and the virtual screen re-fits
each frame.

## Dependencies

Library:

- [`font8x8`](https://crates.io/crates/font8x8) — 8×8 bitmap glyphs (pixel-art text)
- [`fontdue`](https://crates.io/crates/fontdue) — anti-aliased glyph rasterisation
- [`fontdb`](https://crates.io/crates/fontdb) — system-font resolution by family
- [`thiserror`](https://crates.io/crates/thiserror) — typed library errors
- [`serde`](https://crates.io/crates/serde) + [`toml`](https://crates.io/crates/toml) + [`serde_json`](https://crates.io/crates/serde_json) — config (de)serialization

Optional `ratgames` feature:

- [`minifb`](https://crates.io/crates/minifb) — native window + raw `u32`
  framebuffer backend exposed as `ratgames::host::minifb::MinifbHost`

Development / consumers:

- [`anyhow`](https://crates.io/crates/anyhow) — error handling in examples and
  the `mathgame-app` binary

`mathgame-core` is intentionally dependency-free. `mathgame-app` depends on
`mathgame-core`, `ratgames` with the `minifb` feature enabled, and small
app-local config/error crates (`serde`, `toml`, `serde_json`, `thiserror`,
`anyhow`).

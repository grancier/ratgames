# mazegame — browser shell

Runs [`mazegame-app`](../mazegame-app) in the browser via WebAssembly. The game
(rules, screens, rendering) is the **same Rust the native binary runs**; this
directory is only the ~100-line shell the browser owns — the canvas, the
keyboard, and the `requestAnimationFrame` loop — plus the build/serve tooling.
It lives outside the Cargo workspace and depends on it only through the built
`.wasm`.

## How it fits together

```
mazegame-web (cdylib)        wasm-bindgen #[wasm_bindgen] shell: start(canvas, seed)
  └─ mazegame-app (lib)      the shared game construction (Ctx + ScreenStack)
       └─ ratgames [wasm]    WasmHost blits the Surface to <canvas>; embedded font
```

`start()` builds the game from `AppConfig::bundled_for_web()` — the shipped
config with its 32px raster text rendered through the crate-bundled DejaVu Sans
Mono **Bold** (`FontSource::Embedded`), since the browser has no system fonts.

## Prerequisites

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.126 --locked   # match the crate
# wasm-opt (Binaryen) is only needed for --release
npm install
```

## Build & run

```sh
npm run build          # debug bundle -> dist/
npm run build:release  # wasm-opt -O3 + minified JS
npm run serve          # http://localhost:8080  (application/wasm MIME, CSP)
npm run dev            # build + serve
npm run typecheck      # tsc --noEmit
```

`dist/` and `generated/` are build output (git-ignored). `serve.mjs` is a local
preview server; for production, serve the immutable, content-hashed assets from
a CDN (or Node/Fastify) with `Content-Type: application/wasm`,
`Cache-Control: immutable` on hashed files, `no-cache` on `index.html`, and the
CSP `serve.mjs` sets.

## Controls

Arrow keys move one tile per press; `R` restarts the level, `N` re-deals it,
Enter advances a cleared level, Esc quits.

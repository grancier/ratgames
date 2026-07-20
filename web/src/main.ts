// The whole browser shell for mazegame. Everything gameplay lives in the
// WebAssembly module (the same Rust the native binary runs); this file only
// owns what the browser owns: the canvas size, the keyboard, and the
// requestAnimationFrame loop that drives one frame at a time.

import init, { start, type Game } from "../generated/mazegame_web.js";

const canvas = document.getElementById("screen") as HTMLCanvasElement;
const errorBox = document.getElementById("error") as HTMLPreElement;

// Cap the framebuffer's longest edge. The CPU pipeline (upscale + ARGB→RGBA8
// swizzle + putImageData) is O(pixels) *per frame*, so a device-resolution
// buffer on a large hi-DPI display makes each frame heavy enough to drop the
// frame rate — and input, applied once per frame, then feels laggy. 1280 is an
// ample 2× of the 640×360 screen; the browser scales the canvas up to the CSS
// size for free, and `image-rendering: pixelated` keeps it crisp.
const MAX_BACKING_EDGE = 1280;

/** Fill the viewport (CSS), but render into a *capped* backing store. The Rust
 *  `Presentation` integer-upscales the 16:9 screen into it and letterboxes the
 *  remainder; the browser then scales that small buffer up to the CSS size. */
function sizeCanvas(): void {
  const dpr = window.devicePixelRatio || 1;
  const cssW = Math.max(1, window.innerWidth);
  const cssH = Math.max(1, window.innerHeight);
  canvas.style.width = `${cssW}px`;
  canvas.style.height = `${cssH}px`;

  const deviceW = cssW * dpr;
  const deviceH = cssH * dpr;
  const cap = Math.min(1, MAX_BACKING_EDGE / Math.max(deviceW, deviceH));
  canvas.width = Math.max(1, Math.round(deviceW * cap));
  canvas.height = Math.max(1, Math.round(deviceH * cap));
}

/** Arrow keys and space scroll the page by default; the game consumes them. */
const SWALLOW = new Set([
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  " ",
]);

function fail(message: unknown): void {
  errorBox.textContent = `mazegame failed to start:\n\n${String(message)}`;
  errorBox.classList.add("is-visible");
  // eslint-disable-next-line no-console
  console.error(message);
}

async function main(): Promise<void> {
  // `--target web` glue: load and instantiate the .wasm sitting next to the
  // bundle. Passing the URL explicitly keeps it correct after bundling.
  await init(new URL("./mazegame_web_bg.wasm", import.meta.url));

  sizeCanvas();
  window.addEventListener("resize", sizeCanvas);

  // wasm has no wall clock; hand it one to seed the maze deal.
  const game: Game = start(canvas, Date.now());

  window.addEventListener("keydown", (event) => {
    game.on_key(event.key);
    if (SWALLOW.has(event.key)) event.preventDefault();
  });

  const loop = (): void => {
    try {
      game.frame();
    } catch (err) {
      fail(err);
      return;
    }
    if (!game.quit) requestAnimationFrame(loop);
  };
  requestAnimationFrame(loop);
}

main().catch(fail);

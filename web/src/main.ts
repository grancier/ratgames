// The whole browser shell for mazegame. Everything gameplay lives in the
// WebAssembly module (the same Rust the native binary runs); this file only
// owns what the browser owns: the canvas size, the keyboard, and the
// requestAnimationFrame loop that drives one frame at a time.

import init, { start, type Game } from "../generated/mazegame_web.js";

const canvas = document.getElementById("screen") as HTMLCanvasElement;
const errorBox = document.getElementById("error") as HTMLPreElement;

// Cap the integer render scale. The CPU pipeline (upscale + ARGB→RGBA8 swizzle +
// putImageData) is O(pixels) *per frame*, so a device-resolution buffer on a
// large hi-DPI display makes each frame heavy enough to drop the frame rate —
// and input, applied once per frame, then feels laggy. 3× of the virtual screen
// (≤ 1920×1080) is plenty; the browser scales the canvas to the CSS size for
// free, and `image-rendering: pixelated` keeps it crisp.
const MAX_SCALE = 3;

/** Size the canvas from the game's own virtual dimensions. The backing store is
 *  the virtual screen at the largest integer scale that fits (capped), so the
 *  `Presentation` upscale fills it edge-to-edge with no wasted letterbox; the CSS
 *  size is that same aspect contain-fit into the viewport, and the browser scales
 *  the small buffer up. Sizing the backing to the *viewport* aspect instead
 *  mismatched the integer scale and shrank the picture to a small band. */
function sizeCanvas(vw: number, vh: number): void {
  const dpr = window.devicePixelRatio || 1;
  // Contain-fit the virtual screen into the viewport (CSS pixels).
  const fit = Math.min(window.innerWidth / vw, window.innerHeight / vh);
  canvas.style.width = `${Math.max(1, Math.floor(vw * fit))}px`;
  canvas.style.height = `${Math.max(1, Math.floor(vh * fit))}px`;
  // Backing store: virtual × an integer scale near the display resolution,
  // capped. An exact integer multiple keeps the game filling the canvas.
  const scale = Math.max(1, Math.min(MAX_SCALE, Math.floor(fit * dpr)));
  canvas.width = vw * scale;
  canvas.height = vh * scale;
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

  // wasm has no wall clock; hand it one to seed the maze deal.
  const game: Game = start(canvas, Date.now());

  // Size the canvas from the game's own virtual dimensions (the next frame
  // adapts the framebuffer to it).
  const resize = (): void => sizeCanvas(game.width, game.height);
  resize();
  window.addEventListener("resize", resize);

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

// The whole browser shell for mazegame. Everything gameplay lives in the
// WebAssembly module (the same Rust the native binary runs); this file only
// owns what the browser owns: the canvas, the keyboard, and *when* to draw.
//
// mazegame is turn-based — it changes only on a keypress — so this shell renders
// **on demand** (once per input) rather than on a continuous 60 fps loop. A
// constant loop re-ran the whole CPU pipeline (upscale → swizzle → ImageData →
// putImageData) every frame, saturating the main thread so keydown events queued
// behind in-flight frames; that was the input lag. It also renders into a small
// fixed-size buffer and lets the browser scale the canvas up (retro "render
// small, scale up"), so frame cost is cheap and independent of display size.

import init, { start, type Game } from "../generated/mazegame_web.js";

const canvas = document.getElementById("screen") as HTMLCanvasElement;
const errorBox = document.getElementById("error") as HTMLPreElement;

// Backing store = virtual screen × this fixed factor. It does NOT track the
// display: the browser scales the canvas to its CSS size (image-rendering:
// pixelated keeps it crisp), so a large / hi-DPI screen no longer inflates the
// per-frame CPU cost. 2× keeps the HUD text legible while staying cheap.
const RENDER_SCALE = 2;

/** Arrow keys and space scroll the page by default; the game consumes them. */
const SWALLOW = new Set(["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", " "]);

function fail(message: unknown): void {
  errorBox.textContent = `mazegame failed to start:\n\n${String(message)}`;
  errorBox.classList.add("is-visible");
  // eslint-disable-next-line no-console
  console.error(message);
}

/** Contain-fit the game's aspect into the viewport in CSS pixels. The backing
 *  store is fixed, so a resize only restyles the canvas — the browser rescales
 *  the already-rendered bitmap, no re-render needed. */
function fitToViewport(vw: number, vh: number): void {
  const fit = Math.min(window.innerWidth / vw, window.innerHeight / vh);
  canvas.style.width = `${Math.max(1, Math.floor(vw * fit))}px`;
  canvas.style.height = `${Math.max(1, Math.floor(vh * fit))}px`;
}

async function main(): Promise<void> {
  // `--target web` glue: load and instantiate the .wasm sitting next to the
  // bundle. Passing the URL explicitly keeps it correct after bundling.
  await init(new URL("./mazegame_web_bg.wasm", import.meta.url));

  // wasm has no wall clock; hand it one to seed the maze deal.
  const game: Game = start(canvas, Date.now());

  // Fixed, modest backing store sized to the game's 16:9 virtual screen, so the
  // Presentation upscale fills it edge-to-edge with no letterbox.
  canvas.width = game.width * RENDER_SCALE;
  canvas.height = game.height * RENDER_SCALE;

  fitToViewport(game.width, game.height);
  window.addEventListener("resize", () => fitToViewport(game.width, game.height));

  // Render only in response to a change, coalesced to at most one draw per
  // animation frame. Between keypresses the main thread is idle, so input is
  // dispatched immediately instead of queuing behind a busy render loop.
  let queued = false;
  const draw = (): void => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => {
      queued = false;
      try {
        game.frame();
      } catch (err) {
        fail(err);
      }
    });
  };

  window.addEventListener("keydown", (event) => {
    if (game.quit) return;
    game.on_key(event.key);
    if (SWALLOW.has(event.key)) event.preventDefault();
    draw();
  });

  draw(); // the opening frame
}

main().catch(fail);

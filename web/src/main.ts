// The whole browser shell for mazegame. Everything gameplay lives in the
// WebAssembly module (the same Rust the native binary runs); this file only
// owns what the browser owns: the canvas size, the keyboard, and the
// requestAnimationFrame loop that drives one frame at a time.

import init, { start, type Game } from "../generated/mazegame_web.js";

const canvas = document.getElementById("screen") as HTMLCanvasElement;
const errorBox = document.getElementById("error") as HTMLPreElement;

/** Size the canvas to fill the viewport, at device resolution for crispness.
 *  The Rust `Presentation` integer-upscales the 16:9 virtual screen into
 *  whatever size it finds and letterboxes the remainder, so filling the viewport
 *  makes the game as large as it can be; a square canvas (the smaller viewport
 *  dimension) instead boxed the wide screen into a small band. */
function sizeCanvas(): void {
  const dpr = window.devicePixelRatio || 1;
  const cssW = Math.max(1, window.innerWidth);
  const cssH = Math.max(1, window.innerHeight);
  canvas.style.width = `${cssW}px`;
  canvas.style.height = `${cssH}px`;
  canvas.width = Math.floor(cssW * dpr);
  canvas.height = Math.floor(cssH * dpr);
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

// The whole browser shell for mazegame. Everything gameplay lives in the
// WebAssembly module (the same Rust the native binary runs); this file only
// owns what the browser owns: the canvas size, the keyboard, and the
// requestAnimationFrame loop that drives one frame at a time.

import init, { start, type Game } from "../generated/mazegame_web.js";

const canvas = document.getElementById("screen") as HTMLCanvasElement;
const errorBox = document.getElementById("error") as HTMLPreElement;

/** Size the canvas backing store to a device-pixel square that fits the
 *  viewport. The Rust side letterboxes the maze into whatever size it finds, so
 *  any size is valid — this just keeps it large and crisp. */
function sizeCanvas(): void {
  const dpr = window.devicePixelRatio || 1;
  const cssSide = Math.max(1, Math.min(window.innerWidth, window.innerHeight));
  canvas.style.width = `${cssSide}px`;
  canvas.style.height = `${cssSide}px`;
  canvas.width = Math.floor(cssSide * dpr);
  canvas.height = Math.floor(cssSide * dpr);
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

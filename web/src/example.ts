// The browser shell for one gallery demo. Everything demo lives in the
// WebAssembly module (the same Rust the native example binaries run); this
// file only owns what the browser owns: the canvas, the keyboard, and *when*
// to draw. Which demo to start comes from the page's `data-demo` attribute —
// the pages are generated from one manifest, so this shell is shared by all.
//
// Two render models, chosen by the demo itself (`demo.animated`): a ticking
// demo (marquee, wave, room slide, feedback beat) runs a continuous
// requestAnimationFrame loop; an input-only demo renders **synchronously in
// the keydown handler** — a keypress is the only thing that changes it, the
// frame is small and cheap, and skipping the rAF defer means zero added
// latency (the next lever the mazegame input-lag fix pointed at). Both render
// into a small fixed backing store and let the browser scale the canvas up
// (retro "render small, scale up").

import init, { start, type Demo } from "../generated/demos/demos_web.js";

const canvas = document.getElementById("screen") as HTMLCanvasElement;
const errorBox = document.getElementById("error") as HTMLPreElement;

// Backing store = virtual screen × this fixed factor; the browser scales the
// canvas to its CSS size (image-rendering: pixelated keeps it crisp).
const RENDER_SCALE = 2;

/** Arrow keys and space scroll the page by default; the demos consume them. */
const SWALLOW = new Set(["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", " "]);

function fail(message: unknown): void {
  errorBox.textContent = `the demo failed to start:\n\n${String(message)}`;
  errorBox.classList.add("is-visible");
  // eslint-disable-next-line no-console
  console.error(message);
}

/** Contain-fit the demo's aspect into the viewport in CSS pixels. The backing
 *  store is fixed, so a resize only restyles the canvas — the browser rescales
 *  the already-rendered bitmap, no re-render needed. */
function fitToViewport(vw: number, vh: number): void {
  const fit = Math.min(window.innerWidth / vw, window.innerHeight / vh);
  canvas.style.width = `${Math.max(1, Math.floor(vw * fit))}px`;
  canvas.style.height = `${Math.max(1, Math.floor(vh * fit))}px`;
}

/** Esc (the demo's quit) returns to the gallery menu. */
function exitIfQuit(demo: Demo): boolean {
  if (demo.quit) {
    window.location.assign("../");
    return true;
  }
  return false;
}

async function main(): Promise<void> {
  const slug = document.body.dataset.demo;
  if (!slug) throw new Error("the page names no demo (data-demo)");

  // `--target web` glue: load and instantiate the .wasm sitting next to the
  // bundle. Passing the URL explicitly keeps it correct after bundling.
  await init(new URL("./demos_web_bg.wasm", import.meta.url));
  const demo: Demo = start(slug, canvas);

  // Fixed, modest backing store sized to the demo's virtual screen, so the
  // Presentation upscale fills it edge-to-edge with no letterbox.
  canvas.width = demo.width * RENDER_SCALE;
  canvas.height = demo.height * RENDER_SCALE;

  fitToViewport(demo.width, demo.height);
  window.addEventListener("resize", () => fitToViewport(demo.width, demo.height));

  // One synchronous frame: apply the queued input, tick, blit.
  const draw = (): void => {
    try {
      demo.frame();
    } catch (err) {
      fail(err);
      return;
    }
    exitIfQuit(demo);
  };

  window.addEventListener("keydown", (event) => {
    if (demo.quit) return;
    demo.on_key(event.key);
    if (SWALLOW.has(event.key)) event.preventDefault();
    // An input-only demo redraws right here, in the handler.
    if (!demo.animated) draw();
  });

  if (demo.animated) {
    // A ticking demo owns the clock: one frame per animation frame until quit.
    const loop = (): void => {
      try {
        demo.frame();
      } catch (err) {
        fail(err);
        return;
      }
      if (!exitIfQuit(demo)) requestAnimationFrame(loop);
    };
    requestAnimationFrame(loop);
  } else {
    draw(); // the opening frame
  }
}

main().catch(fail);

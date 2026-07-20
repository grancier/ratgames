// Build the browser bundle: compile the wasm crate, run wasm-bindgen (and
// wasm-opt on release), then bundle the TS shell and stage everything in dist/.
//
//   node build.mjs            # debug
//   node build.mjs --release  # optimized (wasm-opt -O3, minified JS)
//
// Requires the wasm-bindgen CLI pinned to the crate version:
//   cargo install wasm-bindgen-cli --version 0.2.126 --locked
// and the wasm32 target: `rustup target add wasm32-unknown-unknown`.

import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync, copyFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";
import * as sass from "sass";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, ".."); // the cargo workspace root
const release = process.argv.includes("--release");
const profile = release ? "release" : "debug";

const generated = resolve(here, "generated");
const dist = resolve(here, "dist");

const run = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, { stdio: "inherit", ...opts });

// 1. Compile the wasm crate.
run("cargo", [
  "build",
  "-p",
  "mazegame-web",
  "--target",
  "wasm32-unknown-unknown",
  "--locked",
  ...(release ? ["--release"] : []),
], { cwd: root });

// 2. wasm-bindgen: emit the JS glue + processed .wasm into generated/.
rmSync(generated, { recursive: true, force: true });
run("wasm-bindgen", [
  "--target",
  "web",
  "--out-dir",
  generated,
  resolve(root, `target/wasm32-unknown-unknown/${profile}/mazegame_web.wasm`),
]);

// 3. wasm-opt on release (a measured -O3, not -Oz — do not shrink hot kernels).
const bg = resolve(generated, "mazegame_web_bg.wasm");
if (release) {
  run("wasm-opt", ["-O3", bg, "-o", bg]);
}

// 4. Bundle the TS shell and stage the static site in dist/.
rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });
await esbuild.build({
  entryPoints: [resolve(here, "src/main.ts")],
  bundle: true,
  format: "esm",
  target: "es2022",
  outfile: resolve(dist, "main.js"),
  minify: release,
  sourcemap: !release,
});
copyFileSync(resolve(here, "index.html"), resolve(dist, "index.html"));
copyFileSync(bg, resolve(dist, "mazegame_web_bg.wasm"));

// Compile the modular SCSS to the linked stylesheet (compressed on release).
const styles = sass.compile(resolve(here, "styles/main.scss"), {
  style: release ? "compressed" : "expanded",
});
writeFileSync(resolve(dist, "styles.css"), styles.css);

console.log(`\nBuilt ${profile} bundle -> ${dist}`);

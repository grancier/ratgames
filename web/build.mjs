// Build the browser bundles: compile the wasm crates, run wasm-bindgen (and
// wasm-opt on release), bundle the TS shells, generate the /examples gallery
// pages from the manifest, and stage the whole static site in dist/.
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
import { DEMOS, menuPage, playerPage } from "./gallery.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, ".."); // the cargo workspace root
const release = process.argv.includes("--release");
const profile = release ? "release" : "debug";

const generated = resolve(here, "generated");
const dist = resolve(here, "dist");

const run = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, { stdio: "inherit", ...opts });

// Compile a wasm crate, emit its JS glue + processed .wasm into `outDir`, and
// wasm-opt the binary on release (a measured -O3, not -Oz — do not shrink hot
// kernels). Returns the path of the processed .wasm.
function wasmCrate(pkg, outDir) {
  run(
    "cargo",
    [
      "build",
      "-p",
      pkg,
      "--target",
      "wasm32-unknown-unknown",
      "--locked",
      ...(release ? ["--release"] : []),
    ],
    { cwd: root },
  );
  const snake = pkg.replaceAll("-", "_");
  run("wasm-bindgen", [
    "--target",
    "web",
    "--out-dir",
    outDir,
    resolve(root, `target/wasm32-unknown-unknown/${profile}/${snake}.wasm`),
  ]);
  const bg = resolve(outDir, `${snake}_bg.wasm`);
  if (release) {
    run("wasm-opt", ["-O3", bg, "-o", bg]);
  }
  return bg;
}

// 1. The wasm modules: mazegame at the site root, the demo gallery beside it.
rmSync(generated, { recursive: true, force: true });
const mazeWasm = wasmCrate("mazegame-web", generated);
const demosWasm = wasmCrate("demos-web", resolve(generated, "demos"));

// 2. Bundle the TS shells and stage the static site in dist/.
rmSync(dist, { recursive: true, force: true });
mkdirSync(resolve(dist, "examples"), { recursive: true });
const bundle = (entry, outfile) =>
  esbuild.build({
    entryPoints: [resolve(here, entry)],
    bundle: true,
    format: "esm",
    target: "es2022",
    outfile: resolve(dist, outfile),
    minify: release,
    sourcemap: !release,
  });
await bundle("src/main.ts", "main.js");
await bundle("src/example.ts", "examples/examples.js");

copyFileSync(resolve(here, "index.html"), resolve(dist, "index.html"));
copyFileSync(mazeWasm, resolve(dist, "mazegame_web_bg.wasm"));
copyFileSync(demosWasm, resolve(dist, "examples/demos_web_bg.wasm"));

// 3. The gallery pages, generated from the one manifest: the menu and a
// player page per demo (dist/examples/<slug>/index.html).
writeFileSync(resolve(dist, "examples/index.html"), menuPage());
for (const demo of DEMOS) {
  const dir = resolve(dist, "examples", demo.slug);
  mkdirSync(dir, { recursive: true });
  writeFileSync(resolve(dir, "index.html"), playerPage(demo));
}

// 4. Compile the modular SCSS to the linked stylesheet (compressed on release).
const styles = sass.compile(resolve(here, "styles/main.scss"), {
  style: release ? "compressed" : "expanded",
});
writeFileSync(resolve(dist, "styles.css"), styles.css);

console.log(`\nBuilt ${profile} bundle -> ${dist}`);

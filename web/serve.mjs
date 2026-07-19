// A tiny static server for dist/ with the response policies a wasm site needs:
// the application/wasm MIME type (so instantiateStreaming works), a no-cache
// index, and a conservative CSP that permits wasm compilation but not eval.
// For production, front immutable hashed assets with a CDN; this is for local
// preview and as the reference for a Node/Fastify deployment.
//
//   node serve.mjs           # http://localhost:8080
//   PORT=3000 node serve.mjs

import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { dirname, extname, join, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const dist = resolve(here, "dist");
const port = Number(process.env.PORT) || 8080;

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".wasm": "application/wasm",
  ".map": "application/json; charset=utf-8",
  ".json": "application/json; charset=utf-8",
};

const CSP =
  "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; " +
  "worker-src 'self'; object-src 'none'; base-uri 'none'";

const server = createServer(async (req, res) => {
  // Resolve the request to a path inside dist/, defaulting to index.html.
  const urlPath = decodeURIComponent((req.url || "/").split("?")[0]);
  const rel = normalize(urlPath).replace(/^(\.\.[/\\])+/, "");
  let file = join(dist, rel === "/" ? "index.html" : rel);
  if (!file.startsWith(dist)) {
    res.writeHead(403).end("Forbidden");
    return;
  }

  try {
    const body = await readFile(file);
    const ext = extname(file);
    const isHtml = ext === ".html";
    res.writeHead(200, {
      "Content-Type": TYPES[ext] || "application/octet-stream",
      "Content-Security-Policy": CSP,
      // Immutable for hashed assets in production; here nothing is hashed, so
      // keep it simple and never cache during local iteration.
      "Cache-Control": isHtml ? "no-cache" : "no-store",
    });
    res.end(body);
  } catch {
    res.writeHead(404, { "Content-Type": "text/plain" }).end("Not found");
  }
});

server.listen(port, () => {
  console.log(`serving ${dist} at http://localhost:${port}`);
});

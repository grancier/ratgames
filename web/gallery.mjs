// The /examples gallery, as data: one entry per demo — the slug is both the
// route segment (/examples/<slug>/) and the name the wasm module resolves —
// plus the two page templates build.mjs renders from it. Build-time only:
// nothing in this file ships to the browser except the HTML it produces.

export const DEMOS = [
  {
    slug: "marquee",
    title: "Marquee",
    blurb: "A scrolling oversized-text banner over the anti-aliased input field.",
    keys: "Type · Enter submits · Esc ends",
  },
  {
    slug: "text_input",
    title: "Text Input",
    blurb: "Type a line; Enter bakes it into a drop-shadowed banner.",
    keys: "Type · Enter bakes · Esc ends",
  },
  {
    slug: "level_complete",
    title: "Level Complete",
    blurb: "Press Enter to reveal the YOU WIN! card.",
    keys: "Enter reveals, then dismisses · Esc ends",
  },
  {
    slug: "high_score",
    title: "High Scores",
    blurb: "The ranked board, loaded through the storage seam.",
    keys: "Enter or Esc ends",
  },
  {
    slug: "arrow_nav",
    title: "Arrow Nav",
    blurb: "Steer a block cell-by-cell; the grid's edges hold it in.",
    keys: "Arrows move · Esc ends",
  },
  {
    slug: "room_scroll",
    title: "Room Scroll",
    blurb: "Steer between four lettered rooms; the screen slides to the neighbour.",
    keys: "Arrows move · Esc ends",
  },
  {
    slug: "text_wave",
    title: "Text Wave",
    blurb: "A line of big pixel text ripples up and back down.",
    keys: "Esc ends",
  },
  {
    slug: "math_game",
    title: "Math Game",
    blurb: "The worked arcade quiz: answer, keep your lives, clear both levels.",
    keys: "Type digits · Enter submits · Esc ends",
  },
];

/** The shared document skeleton; `stylesheet` is page-relative. */
const page = (title, stylesheet, bodyAttrs, body) => `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
    <title>${title}</title>
    <link rel="stylesheet" href="${stylesheet}" />
  </head>
  <body${bodyAttrs}>
${body}
  </body>
</html>
`;

/** The /examples/ menu: every demo as a card linking to its player page. */
export function menuPage() {
  const cards = DEMOS.map(
    (demo) => `      <li>
        <a href="./${demo.slug}/">
          <span class="demo-title">${demo.title}</span>
          <span class="demo-blurb">${demo.blurb}</span>
          <span class="demo-keys">${demo.keys}</span>
        </a>
      </li>`,
  ).join("\n");
  return page(
    "ratgames examples",
    "../styles.css",
    ' class="gallery-page"',
    `    <main class="gallery">
      <h1>RATGAMES EXAMPLES</h1>
      <p class="gallery-tag">The toolkit's demos, running in the browser through the same Rust the native binaries run.</p>
      <ul>
${cards}
      </ul>
    </main>`,
  );
}

/** One playable page: the canvas, the demo slug for the shell, small chrome. */
export function playerPage(demo) {
  return page(
    `ratgames: ${demo.title.toLowerCase()}`,
    "../../styles.css",
    ` data-demo="${demo.slug}"`,
    `    <a class="back-link" href="../">&larr; examples</a>
    <p class="demo-help">${demo.keys}</p>
    <canvas id="screen"></canvas>
    <pre id="error"></pre>
    <script type="module" src="../examples.js"></script>`,
  );
}

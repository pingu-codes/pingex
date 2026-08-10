/** Build `demo/screenshots/index.html` — a contact sheet of every captured
 * shot, light and dark side by side, so a demo set can be reviewed in one
 * place. Run after the capture: `deno task demo`. */

const ROOT = new URL("./screenshots/", import.meta.url);

function shots(theme: string): string[] {
  try {
    return [...Deno.readDirSync(new URL(`${theme}/`, ROOT))]
      .filter((entry) => entry.isFile && entry.name.endsWith(".png"))
      .map((entry) => entry.name)
      .sort();
  } catch {
    return [];
  }
}

const light = shots("light");
const dark = new Set(shots("dark"));
if (light.length === 0) {
  console.error("No screenshots found — run `deno task demo:capture` first.");
  Deno.exit(1);
}

const title = (name: string) =>
  name
    .replace(/\.png$/, "")
    .replace(/^\d+-/, "")
    .replace(/-/g, " ")
    .replace(/^./, (character) => character.toUpperCase());

const rows = light
  .map((name) => {
    const pair = [
      `<figure><img src="light/${name}" alt="${title(name)}, light" /><figcaption>light</figcaption></figure>`,
    ];
    if (dark.has(name)) {
      pair.push(`<figure><img src="dark/${name}" alt="${title(name)}, dark" /><figcaption>dark</figcaption></figure>`);
    }
    return `<section><h2>${title(name)}</h2><div class="pair">${pair.join("")}</div></section>`;
  })
  .join("\n");

const html = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<meta name="color-scheme" content="light dark" />
<title>Pingex — demo screenshots</title>
<style>
  :root { color-scheme: light dark; }
  body { margin: 0 auto; max-width: 1400px; padding: 3rem 1.5rem 6rem; font: 15px/1.5 -apple-system, system-ui, sans-serif; }
  h1 { font-size: 1.6rem; letter-spacing: -0.02em; }
  p.lede { color: color-mix(in srgb, currentColor 60%, transparent); margin-bottom: 3rem; }
  section { margin-bottom: 3.5rem; }
  h2 { font-size: 1rem; letter-spacing: -0.01em; margin-bottom: 0.75rem; }
  .pair { display: grid; gap: 1rem; grid-template-columns: repeat(auto-fit, minmax(420px, 1fr)); }
  figure { margin: 0; }
  img { width: 100%; border-radius: 10px; border: 1px solid color-mix(in srgb, currentColor 18%, transparent); }
  figcaption { margin-top: 0.4rem; font-size: 12px; color: color-mix(in srgb, currentColor 55%, transparent); }
</style>
</head>
<body>
<h1>Pingex — demo screenshots</h1>
<p class="lede">${light.length} shots, captured from the browser preview fixtures. Regenerate with <code>deno task demo</code>.</p>
${rows}
</body>
</html>
`;

Deno.writeTextFileSync(new URL("index.html", ROOT), html);
console.log(`Wrote demo/screenshots/index.html (${light.length} shots).`);

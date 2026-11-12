/**
 * Seam rules `bun run check` holds, beyond the type checker: commands that
 * only one module may call. A PR gets onto disk, and a stack's worktree
 * switches PR, through `src/lib/review/prOnDisk.ts` alone (the preview
 * desk's fakes aside), so every caller shares its adopt dialog, Undo toast,
 * setup and running-thread wait.
 */
import { readdir, readFile } from "node:fs/promises";
import { join, relative } from "node:path";

const SEAMS: { pattern: RegExp; allowed: (path: string) => boolean; owner: string }[] = [
  {
    pattern: /\.(?:reviewCheckOut|reviewCheckOutStack|reviewAdoptWorktree|reviewSwitchStackBranch)\s*\(/,
    allowed: (path) => path === "src/lib/review/prOnDisk.ts" || path.startsWith("src/lib/services/preview/"),
    owner: "src/lib/review/prOnDisk.ts",
  },
];

/** Generated, or tests, which may name anything. */
const SKIPPED = (path: string) => path === "src/lib/bindings.ts" || /\.test\.(ts|svelte\.ts)$/.test(path);

async function* sources(dir: string): AsyncGenerator<string> {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) yield* sources(path);
    else if (/\.(ts|svelte)$/.test(entry.name)) yield path;
  }
}

const root = process.cwd();
const broken: string[] = [];
for await (const file of sources(join(root, "src"))) {
  const path = relative(root, file);
  if (SKIPPED(path)) continue;
  const lines = (await readFile(file, "utf8")).split("\n");
  for (const seam of SEAMS) {
    if (seam.allowed(path)) continue;
    lines.forEach((line, index) => {
      const found = seam.pattern.exec(line);
      if (found) broken.push(`${path}:${index + 1}: ${found[0].slice(1, -1).trim()} is called only from ${seam.owner}`);
    });
  }
}
if (broken.length > 0) {
  console.error(broken.join("\n"));
  process.exit(1);
}

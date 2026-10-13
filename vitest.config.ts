import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";

const lucideDist = resolve(dirname(fileURLToPath(import.meta.url)), "node_modules/@lucide/svelte/dist");

/**
 * Rewrites `import { X, Y as Z } from "@lucide/svelte"` into one import per
 * icon file. The barrel re-exports ~1,900 components, and Vitest evaluates
 * all of them in every test file that renders a component; Vite's build
 * tree-shakes them, so only tests need this. Names the map does not know
 * (`Icon`, types) stay on the barrel import.
 */
function lucideDeepImports(): Plugin {
  const files = new Map<string, string>();
  for (const index of ["icons/index.js", "aliases/aliases.js", "aliases/prefixed.js", "aliases/suffixed.js"]) {
    const dir = dirname(resolve(lucideDist, index));
    const source = readFileSync(resolve(lucideDist, index), "utf8");
    for (const [, name, path] of source.matchAll(/default as (\w+)\s*\}\s*from\s*'([^']+)'/g)) {
      files.set(name, resolve(dir, path));
    }
  }
  const barrelImport = /import\s*\{([^}]*)\}\s*from\s*["']@lucide\/svelte["'];?/g;
  return {
    name: "lucide-deep-imports",
    enforce: "pre",
    transform(code, id) {
      if (id.includes("node_modules") || !code.includes("@lucide/svelte")) return;
      const rewritten = code.replace(barrelImport, (match, specifiers: string) => {
        const deep: string[] = [];
        const rest: string[] = [];
        for (const raw of specifiers.split(",")) {
          const specifier = raw.trim();
          if (!specifier) continue;
          const [name, local = name] = specifier.split(/\s+as\s+/);
          const file = files.get(name);
          if (file) deep.push(`import ${local} from ${JSON.stringify(file)};`);
          else rest.push(specifier);
        }
        if (rest.length) deep.push(`import { ${rest.join(", ")} } from "@lucide/svelte";`);
        // Keep the line count so stack traces still point at the source.
        return deep.join(" ") + "\n".repeat(match.split("\n").length - 1);
      });
      return rewritten === code ? undefined : { code: rewritten, map: null };
    },
  };
}

export default defineConfig({
  plugins: [lucideDeepImports(), svelte(), svelteTesting()],
  resolve: {
    alias: { $lib: new URL("./src/lib", import.meta.url).pathname },
  },
  ssr: {
    noExternal: ["@lucide/svelte", "@skeletonlabs/skeleton-svelte", "@zag-js/svelte"],
  },
  test: {
    environment: "jsdom",
    // Terse output by default: agents re-read every line of test output on
    // every later turn. Failures still print in full.
    reporters: ["dot"],
    silent: "passed-only",
    include: ["src/**/*.test.ts"],
    setupFiles: ["./vitest-setup.ts"],
  },
});

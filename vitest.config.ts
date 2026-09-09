import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [svelte(), svelteTesting()],
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

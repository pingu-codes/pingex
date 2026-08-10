import { defineConfig } from "@playwright/test";

/** Screenshot capture for demos and docs — not part of `deno task test:e2e`.
 * Runs the same browser preview the e2e suite uses, but drives it purely to
 * pose the UI and save images. See `demo/README.md`. */
export default defineConfig({
  testDir: "./demo",
  testMatch: "screenshots.spec.ts",
  globalSetup: "./demo/setup.ts",
  fullyParallel: true,
  // A few workers keep the capture quick without starving the dev server, and
  // one retry covers the occasional cold-start miss on the first page load.
  workers: 4,
  retries: 1,
  // Posing several screens in one test takes longer than an assertion-only run.
  timeout: 60_000,
  reporter: "list",
  outputDir: "test-results/demo",
  use: {
    baseURL: "http://127.0.0.1:1420",
    // Retina-quality images at a comfortable window size.
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 2,
  },
  projects: [
    { name: "light", use: { browserName: "chromium", colorScheme: "light" } },
    { name: "dark", use: { browserName: "chromium", colorScheme: "dark" } },
  ],
  webServer: {
    command: "deno task frontend:dev",
    url: "http://127.0.0.1:1420",
    reuseExistingServer: true,
    timeout: 120_000,
  },
});

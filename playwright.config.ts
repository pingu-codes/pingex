import { defineConfig } from "@playwright/test";

const ci = Boolean((globalThis as typeof globalThis & { process?: { env?: { CI?: string } } }).process?.env?.CI);

export default defineConfig({
  testDir: "./tests/browser",
  fullyParallel: true,
  forbidOnly: ci,
  retries: ci ? 2 : 0,
  reporter: "list",
  outputDir: "test-results",
  use: {
    baseURL: "http://127.0.0.1:1420",
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  projects: [
    { name: "desktop-1180x760", use: { browserName: "chromium", viewport: { width: 1180, height: 760 } } },
    { name: "minimum-820x560", use: { browserName: "chromium", viewport: { width: 820, height: 560 } } },
    { name: "webkit", use: { browserName: "webkit", viewport: { width: 1180, height: 760 } } },
  ],
  webServer: {
    command: "deno task frontend:dev",
    url: "http://127.0.0.1:1420",
    reuseExistingServer: !ci,
    timeout: 120_000,
  },
});

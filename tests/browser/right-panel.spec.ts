import { expect, type Page, test } from "@playwright/test";

async function loadPreview(page: Page) {
  await page.goto("/");
  await expect(page.getByText("codex-custom", { exact: true }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "New thread", exact: true })).toBeVisible();
}

async function selectPreviewThread(page: Page) {
  await page.getByRole("button", { name: /Custom frontend skeleton/ }).click();
  await expect(page.getByRole("button", { name: "Worked for 24s" })).toBeVisible();
}

function rightPanel(page: Page) {
  return page.getByRole("complementary", { name: "Thread side panel" });
}

// The overview card floats over the top of the right panel, so tests collapse
// it before interacting with panel content, like a user would.
async function closeOverview(page: Page) {
  await page.getByRole("button", { name: "Thread overview" }).click();
}

test.beforeEach(async ({ page }) => {
  await loadPreview(page);
  await selectPreviewThread(page);
});

test("keeps the overview in a short viewport and scrolls its contents", async ({ page }) => {
  await page.setViewportSize({ width: 820, height: 560 });

  const overview = page.getByRole("menu", { name: "Thread overview panel" });
  await expect(overview).toBeVisible();

  const [bounds, viewport] = await Promise.all([overview.boundingBox(), page.viewportSize()]);
  if (!bounds || !viewport) throw new Error("Expected overview and viewport bounds");
  expect(bounds.y + bounds.height).toBeLessThanOrEqual(viewport.height);

  const scroll = await overview.evaluate((element) => ({
    clientHeight: element.clientHeight,
    scrollHeight: element.scrollHeight,
    overflowY: getComputedStyle(element).overflowY,
  }));
  expect(scroll.overflowY).toBe("auto");
  expect(scroll.scrollHeight).toBeGreaterThan(scroll.clientHeight);
});

test("browses the complete file tree from the Files button", async ({ page }) => {
  await page.getByRole("button", { name: "Files", exact: true }).click();
  await closeOverview(page);

  const panel = rightPanel(page);
  await expect(panel.getByText("Files")).toBeVisible();
  await expect(panel.getByText("README.md")).toBeVisible();
  await expect(panel.getByText("api.ts")).not.toBeVisible();

  await panel.getByRole("button", { name: "src", exact: true }).click();
  await expect(panel.getByText("App.svelte")).toBeVisible();

  await panel.getByRole("button", { name: "lib", exact: true }).click();
  await expect(panel.getByText("api.ts")).toBeVisible();
  await expect(panel.getByText("utils.ts")).toBeVisible();

  // Collapsing the folder hides its children again.
  await panel.getByRole("button", { name: "src", exact: true }).click();
  await expect(panel.getByText("api.ts")).not.toBeVisible();
});

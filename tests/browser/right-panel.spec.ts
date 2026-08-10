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

test("opens a diff from the overview outputs in the right panel", async ({ page }) => {
  await page.getByTitle("View diff for src/lib/utils.ts").click();
  await closeOverview(page);

  const panel = rightPanel(page);
  await expect(panel.getByText("Changes")).toBeVisible();
  await expect(panel.getByText("src/lib/utils.ts")).toBeVisible();
  await expect(panel.getByText("src/lib/generated.ts")).toBeVisible();
  // The small diff is expanded by default; its added lines are readable.
  await expect(panel.getByText("+export function debounce", { exact: false })).toBeVisible();

  // The oversized diff stays collapsed until opened, then offers "show all".
  await panel.getByRole("button", { name: /src\/lib\/generated\.ts/ }).click();
  await expect(panel.getByRole("button", { name: /Show all 401 lines/ })).toBeVisible();

  await panel.getByRole("button", { name: "Close panel" }).click();
  await expect(rightPanel(page)).not.toBeVisible();
});

test("browses the complete file tree from the Files button", async ({ page }) => {
  await page.getByRole("button", { name: "Files" }).click();
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

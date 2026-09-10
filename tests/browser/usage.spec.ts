import { expect, type Page, test } from "@playwright/test";

async function loadPreview(page: Page) {
  await page.goto("/");
  await expect(page.getByText("codex-custom", { exact: true }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "New thread", exact: true })).toBeVisible();
}

test.beforeEach(async ({ page }) => {
  await loadPreview(page);
});

test("the overview's usage row opens the status panel with both breakdowns", async ({ page }) => {
  await page.getByRole("button", { name: /Custom frontend skeleton/ }).click();
  await expect(page.getByRole("button", { name: "Worked for 24s" })).toBeVisible();

  const overview = page.getByRole("menu", { name: "Thread overview panel" });
  // The figures no longer sit in the overview; the row is the way to them.
  await expect(overview.getByText("Thread tokens")).toHaveCount(0);
  await overview.getByRole("button", { name: "Show usage" }).click();
  await page.getByRole("button", { name: "Thread overview" }).click();

  const panel = page.getByRole("complementary", { name: "Thread side panel" });
  await expect(panel.getByText("Status")).toBeVisible();

  // The preview thread runs on Codex, which cannot report its context, so the
  // composition is the estimate.
  const composition = panel.getByTestId("context-composition");
  await expect(composition).toContainText("≈ estimated");
  await expect(composition).toContainText("System prompt");
  await expect(composition).toContainText("Tool use");

  // The system prompt opens into its parts, sized from the rollout file.
  await composition.getByRole("button", { name: "What is in the system prompt" }).click();
  const parts = composition.getByTestId("prompt-parts");
  await expect(parts).toContainText("Base instructions");
  await expect(parts).toContainText("AGENTS.md");
  await expect(parts).toContainText("Tool definitions and other");

  const spend = panel.getByTestId("spend-by-category");
  await expect(spend).toContainText("Spend by category");
  await expect(spend).toContainText("Reasoning");
  await expect(spend).toContainText("≈ estimated from message sizes");
  await expect(spend.getByTestId("usage-cost")).toContainText("≈ $");
});

// The preview opens on the home page, so the usage card is reachable without
// navigating; project details open from the sidebar row's context menu.
test("project details gains a Usage tab with a range and the heaviest threads", async ({ page }) => {
  await page.locator('[data-sidebar-row="item:/Users/ciaran/Projects/codex-custom"]').click({ button: "right" });
  await page.getByRole("menuitem", { name: "Project details", exact: true }).click();
  await page.getByRole("tab", { name: "Usage" }).click();

  const summary = page.getByTestId("usage-summary");
  await expect(summary).toContainText("Spend by category");
  await expect(summary.getByRole("button", { name: "30 days" })).toBeVisible();
  await expect(summary).toContainText("Heaviest threads");
  await expect(summary).toContainText("Fix trailing-edge debounce");
  await expect(summary).toContainText("claude-haiku-4-5");

  // A narrower range re-reads; the preview halves its figures for one.
  const before = await summary.getByTestId("usage-cost").textContent();
  await summary.getByRole("button", { name: "7 days" }).click();
  await expect(summary.getByTestId("usage-cost")).not.toHaveText(before ?? "");
});

test("the homepage card links to the global usage section in Settings", async ({ page }) => {
  const card = page.getByTestId("home-usage-card");
  await expect(card).toContainText("Usage · last 30 days");
  await expect(card).toContainText("Turns");
  await card.getByRole("button", { name: "Details" }).click();

  await expect(page.getByRole("tab", { name: "Usage", selected: true })).toBeVisible();
  const summary = page.getByTestId("usage-summary");
  await expect(summary).toContainText("Spend by category");
  await expect(summary).toContainText("Heaviest threads");
});

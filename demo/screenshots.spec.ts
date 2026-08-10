import { expect, test } from "@playwright/test";
import { closeOverview, composer, newThread, open, openDemoThread, openWorktrees, shoot } from "./shoot";

test.beforeEach(async ({ page }) => open(page));

test("home", async ({ page }, info) => {
  await shoot(page, info, "01-home");
});

test("thread transcript", async ({ page }, info) => {
  await openDemoThread(page);
  await closeOverview(page);
  await shoot(page, info, "02-thread");

  await page.getByRole("button", { name: "Worked for 24s" }).click();
  await expect(page.getByText("Planning the helper", { exact: false })).toBeVisible();
  await shoot(page, info, "03-thread-reasoning");

  await page.locator("button").filter({ hasText: "rg -n" }).first().click();
  await expect(page.getByText("export function clamp", { exact: false })).toBeVisible();
  await shoot(page, info, "04-thread-command-output");
});

test("diffs in the side panel", async ({ page }, info) => {
  await openDemoThread(page);
  await page.getByTitle("View diff for src/lib/utils.ts").click();
  await closeOverview(page);
  const panel = page.getByRole("complementary", { name: "Thread side panel" });
  await expect(panel.getByText("+export function debounce", { exact: false })).toBeVisible();
  await shoot(page, info, "05-diff-panel");
});

test("file tree", async ({ page }, info) => {
  await openDemoThread(page);
  await page.getByRole("button", { name: "Files", exact: true }).click();
  await closeOverview(page);
  const panel = page.getByRole("complementary", { name: "Thread side panel" });
  await panel.getByRole("button", { name: "src", exact: true }).click();
  await panel.getByRole("button", { name: "lib", exact: true }).click();
  await expect(panel.getByText("utils.ts")).toBeVisible();
  await shoot(page, info, "06-file-tree");
});

// The overview card opens with the thread, so this shot needs no interaction.
test("thread overview card", async ({ page }, info) => {
  await openDemoThread(page);
  await expect(page.getByRole("menu", { name: "Thread overview panel" })).toBeVisible();
  await shoot(page, info, "07-thread-overview");
});

test("composer file mentions", async ({ page }, info) => {
  await newThread(page);
  await composer(page).fill("Add comments to @util");
  await expect(page.getByRole("option", { name: /utils\.ts/ })).toBeVisible();
  await shoot(page, info, "08-composer-mentions");
});

test("slash commands", async ({ page }, info) => {
  await newThread(page);
  await composer(page).fill("/");
  await expect(page.getByRole("option").first()).toBeVisible();
  await shoot(page, info, "09-slash-commands");
});

test("model and effort picker", async ({ page }, info) => {
  await newThread(page);
  await page.getByRole("button", { name: "Select model and effort" }).click();
  await expect(page.getByText("GPT-5.2 Codex", { exact: false }).first()).toBeVisible();
  await shoot(page, info, "10-model-picker");
});

test("permission levels", async ({ page }, info) => {
  await newThread(page);
  await page.getByRole("button", { name: "Set permissions level" }).click();
  await shoot(page, info, "11-permissions");
});

test("a streaming turn", async ({ page }, info) => {
  await newThread(page);
  await composer(page).fill("Walk me through the debounce helper you added");
  await composer(page).press("Enter");
  await expect(page.getByText("Considering the request", { exact: false })).toBeVisible();
  await closeOverview(page);
  await shoot(page, info, "12-streaming-turn");

  await expect(page.getByRole("button", { name: /Worked for/ })).toBeVisible({ timeout: 30_000 });
  await shoot(page, info, "13-completed-turn");
});

test("an approval request", async ({ page }, info) => {
  await newThread(page);
  await composer(page).fill("Run the migration script, approve as needed");
  await composer(page).press("Enter");
  await expect(page.getByText("Codex wants to run a command")).toBeVisible();
  await closeOverview(page);
  await shoot(page, info, "14-approval-request");
});

test("thread search", async ({ page }, info) => {
  await page.getByRole("button", { name: "Search threads" }).first().click();
  await page.getByRole("textbox", { name: "Search threads" }).fill("a");
  await expect(page.getByText("Tauri app-server bridge").first()).toBeVisible();
  await shoot(page, info, "15-thread-search");
});

test("archived threads", async ({ page }, info) => {
  await page.getByRole("button", { name: /^Archived/ }).click();
  await expect(page.getByText("Old research thread").first()).toBeVisible();
  await shoot(page, info, "16-archived-threads");
});

test("thread context menu", async ({ page }, info) => {
  await page
    .getByRole("button", { name: /Custom frontend skeleton/ })
    .locator("..")
    .getByRole("button", { name: "Thread menu" })
    .click();
  await expect(page.getByRole("menuitem", { name: "Fork thread" })).toBeVisible();
  await shoot(page, info, "17-thread-menu");
});

test("git worktrees", async ({ page }, info) => {
  await openWorktrees(page);
  await expect(page.getByText("search-ranking").first()).toBeVisible();
  await shoot(page, info, "18-worktrees");
});

test("pull-request review", async ({ page }, info) => {
  await openWorktrees(page);
  await page.getByRole("button", { name: "Review", exact: true }).click();
  await expect(page.getByText("Add pull-request review view").first()).toBeVisible();
  await shoot(page, info, "19-review-list");

  await page.getByText("Add pull-request review view").first().click();
  await expect(page.getByText("src/lib/loader.ts").first()).toBeVisible();
  await shoot(page, info, "20-review-diff");
});

test("project details and workspace search", async ({ page }, info) => {
  await page.getByRole("button", { name: "Project menu" }).first().click();
  await page.getByRole("menuitem", { name: "Project details" }).click();
  await expect(page.getByRole("textbox", { name: "Project instructions" })).toHaveValue(/focused PRs/);
  await shoot(page, info, "21-project-details");

  await page.getByRole("searchbox", { name: "Search workspace" }).fill("utils");
  await expect(page.getByText("src/lib/utils.ts").first()).toBeVisible();
  await shoot(page, info, "22-workspace-search");
});

test("side questions", async ({ page }, info) => {
  await openDemoThread(page);
  await page
    .getByRole("menu", { name: "Thread overview panel" })
    .getByRole("button", { name: /Side questions/ })
    .click();
  await expect(page.getByText("Why trailing edge?").first()).toBeVisible();
  await shoot(page, info, "23-side-questions");
});

test("subagents", async ({ page }, info) => {
  await openDemoThread(page);
  await expect(page.getByRole("button", { name: /Open subagent Scout/ })).toBeVisible();
  await shoot(page, info, "24-subagents");

  await page.getByRole("button", { name: /Open subagent Scout/ }).click();
  await expect(page.getByRole("button", { name: "Back to parent thread" })).toBeVisible();
  await closeOverview(page);
  await shoot(page, info, "25-subagent-thread");
});

test("settings", async ({ page }, info) => {
  await page.getByRole("button", { name: /ciaran@example\.com/ }).click();
  const nav = page.locator('[data-testid="settings-nav-item"]');
  await expect(nav.first()).toBeVisible();
  await shoot(page, info, "26-settings-general");

  for (const [index, section] of [
    ["27-settings-appearance", "Appearance"],
    ["28-settings-agent", "Agent"],
    ["29-settings-integrations", "Integrations"],
    ["30-settings-connections", "Connections"],
    ["31-settings-keyboard", "Keyboard shortcuts"],
  ] as const) {
    await nav.filter({ hasText: section }).first().click();
    await shoot(page, info, index);
  }
});

test("quick chat window", async ({ page }, info) => {
  // Match the real quick-chat window (`src-tauri/src/quick.rs`).
  await page.setViewportSize({ width: 560, height: 260 });
  await page.goto("/?window=quick");
  await expect(page.getByRole("textbox").first()).toBeVisible();
  await page.getByRole("textbox").first().fill("Summarise what changed on main today");
  await shoot(page, info, "32-quick-chat");
});

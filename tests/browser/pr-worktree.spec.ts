// Working on a PR: the page offers it, a worktree comes first, and a draft
// thread opens there. In the preview's fakes, so it covers the page's wiring;
// the git behind it is covered by `review/desk/steps.rs`.

import { expect, test } from "@playwright/test";

test("Work on this PR makes a worktree and opens a draft thread there", async ({ page }) => {
  await page.setViewportSize({ width: 1500, height: 800 });
  await page.goto("/");
  await page.getByRole("button", { name: "Pull requests" }).first().click();
  await page.getByText("Add pull-request review view").first().click();

  const work = page.getByRole("button", { name: "Work on this PR" });
  await expect(work).toBeVisible();
  await expect(page.getByRole("img", { name: /^No local worktree/ }).first()).toBeVisible();

  await work.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("needs a worktree");
  await expect(dialog).toContainText("worktrees/");
  await page.screenshot({ path: test.info().outputPath("create-worktree-dialog.png") });
  await dialog.getByRole("button", { name: "Create worktree" }).click();

  await expect(page.getByRole("textbox").first()).toBeVisible();
  await page.screenshot({ path: test.info().outputPath("draft-thread.png") });
});

test("The inbox shows which PRs have a worktree, and says which on hover", async ({ page }) => {
  await page.setViewportSize({ width: 1500, height: 800 });
  await page.goto("/");
  await page.getByRole("button", { name: "Pull requests" }).first().click();

  const row = page.getByRole("listitem", { name: /Add pull-request review view/ });
  const none = row.locator("[data-worktree-state='none']");
  await expect(none).toBeVisible();
  await none.hover();
  // Every row mounts its tooltip hidden, so look for the one showing.
  await expect(page.getByText("Work on this PR creates one.").locator("visible=true")).toBeVisible();
  await page.screenshot({ path: test.info().outputPath("inbox-no-worktree-tooltip.png") });

  // My stack's preview worktree is on its second git branch.
  const onBranch = page.getByRole("listitem", { name: /^#415/ }).locator("[data-worktree-state='here']");
  await expect(onBranch).toBeVisible();
  const lower = page.getByRole("listitem", { name: /^#412/ }).locator("[data-worktree-state='elsewhere']");
  await expect(lower).toBeVisible();
  await lower.hover();
  await expect(
    page.getByText("/Users/ciaran/worktrees/codex-custom/harness-stack").locator("visible=true"),
  ).toBeVisible();
  await expect(page.getByText("not this PR's branch").locator("visible=true")).toBeVisible();
  await page.screenshot({ path: test.info().outputPath("inbox-stack-worktree-tooltip.png") });

  // Rows wrap their badges in a narrow inbox; the icon stays by the number.
  await page.setViewportSize({ width: 820, height: 560 });
  await expect(onBranch).toBeVisible();
  await page.screenshot({ path: test.info().outputPath("inbox-narrow.png") });
});

test("Check out stack makes one worktree, and every row of the stack shows it", async ({ page }) => {
  await page.setViewportSize({ width: 1500, height: 800 });
  await page.goto("/");
  await page.getByRole("button", { name: "Pull requests" }).first().click();

  const stack = page.getByRole("region", { name: /^Stack focus-rings/ });
  await stack.getByRole("button", { name: "Check out stack" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("a local git branch for each of its 2 open PRs");
  await page.screenshot({ path: test.info().outputPath("check-out-stack-dialog.png") });
  await dialog.getByRole("button", { name: "Check out stack" }).click();

  await expect(stack.getByRole("button", { name: "Check out stack" })).toHaveCount(0);
  await expect(stack.getByRole("img", { name: /Worktree jo-r-focus-trap/ }).first()).toBeVisible();
  await expect(page.getByRole("listitem", { name: /^#403/ }).locator("[data-worktree-state='here']")).toBeVisible();
  await expect(
    page.getByRole("listitem", { name: /^#401/ }).locator("[data-worktree-state='elsewhere']"),
  ).toBeVisible();
  await page.screenshot({ path: test.info().outputPath("stack-checked-out.png") });
});

test("The PR and Stack pages say where the code is, and switching a stack PR asks first", async ({ page }) => {
  await page.setViewportSize({ width: 1500, height: 900 });
  await page.goto("/");
  await page.getByRole("button", { name: "Pull requests" }).first().click();
  await page.getByRole("button", { name: "Open stack driver-trait" }).click();

  const panel = page.getByLabel("Worktree harness-stack", { exact: true });
  await expect(panel).toContainText("/Users/ciaran/worktrees/codex-custom/harness-stack");
  await expect(panel).toContainText("On feat/claude-driver");

  // The tree marks which PR the worktree is on; the page's chip names the worktree.
  const lower = page.getByRole("button", { name: /^#412/ });
  await expect(lower.locator("[data-worktree-state='elsewhere']")).toBeVisible();
  await expect(page.getByRole("button", { name: /^#415/ }).locator("[data-worktree-state='here']")).toBeVisible();
  await lower.click();
  await expect(
    page.getByRole("img", { name: /Worktree harness-stack/ }).filter({ hasText: "harness-stack" }),
  ).toBeVisible();
  await page.screenshot({ path: test.info().outputPath("stack-page.png") });

  await page.getByRole("button", { name: "Work on this PR" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("This stack shares one worktree");
  await expect(dialog).toContainText("feat/claude-driver");
  await page.screenshot({ path: test.info().outputPath("switch-worktree-dialog.png") });
});

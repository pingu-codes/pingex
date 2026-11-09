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
  await expect(page.getByText("No checkout", { exact: true }).first()).toBeVisible();

  await work.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("needs a worktree");
  await expect(dialog).toContainText("worktrees/");
  await page.screenshot({ path: test.info().outputPath("create-worktree-dialog.png") });
  await dialog.getByRole("button", { name: "Create worktree" }).click();

  await expect(page.getByRole("textbox").first()).toBeVisible();
  await page.screenshot({ path: test.info().outputPath("draft-thread.png") });
});

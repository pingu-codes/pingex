// Checking a PR out, and what waits for it. In the preview's fakes
// (`services/preview/checkouts.ts`), so it covers the pages' wiring; the git
// behind it is covered by `review/desk/check_out.rs` and `stack_worktree.rs`.

import { expect, type Page, test } from "@playwright/test";

/** Mark the preview desk's stack 7 worktree dirty or clean, as git would see
 *  it, through the module the app itself loaded. */
async function setStackDirty(page: Page, dirty: boolean) {
  await page.evaluate(
    async ([url, value]) => {
      const { desk } = await import(/* @vite-ignore */ url);
      for (const row of desk().checkouts.values()) if (row.stack === 7) row.dirty = value;
    },
    ["/src/lib/services/preview/desk.ts", dirty] as const,
  );
}

async function openInbox(page: Page) {
  await page.setViewportSize({ width: 1500, height: 900 });
  await page.goto("/");
  await page.getByRole("button", { name: "Pull requests" }).first().click();
}

test("Check out makes the PR's worktree, and the work that needs it waits until then", async ({ page }) => {
  await openInbox(page);
  await page.getByText("Native git worktree service").first().click();

  const work = page.getByRole("button", { name: "Work on this PR" });
  await expect(work).toBeDisabled();
  await expect(work).toHaveAttribute("title", "Check out first");
  await expect(page.getByRole("button", { name: "Run locally" })).toBeDisabled();
  await expect(page.getByRole("img", { name: /^Not checked out/ }).first()).toBeVisible();
  await page.screenshot({ path: test.info().outputPath("not-checked-out.png") });

  await page.getByRole("button", { name: "Check out", exact: true }).click();
  await expect(page.getByText("Checked out #126 in pr-126")).toBeVisible();
  await expect(page.getByRole("img", { name: /Worktree pr-126/ }).first()).toBeVisible();
  await expect(work).toBeEnabled();
  await page.screenshot({ path: test.info().outputPath("checked-out.png") });

  await work.click();
  await expect(page.getByRole("textbox").first()).toBeVisible();
});

test("A git branch already checked out somewhere is offered for adoption", async ({ page }) => {
  await openInbox(page);
  await page.getByText("Add pull-request review view").first().click();

  await page.getByRole("button", { name: "Check out", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toContainText("Use pr-review-by-hand for #128?");
  await expect(dialog).toContainText("feature/pr-review");
  await expect(dialog).toContainText("/Users/ciaran/worktrees/codex-custom/pr-review-by-hand");
  await page.screenshot({ path: test.info().outputPath("adopt-dialog.png") });
  await dialog.getByRole("button", { name: "Adopt" }).click();

  await expect(page.getByRole("img", { name: /Worktree pr-review-by-hand/ }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "Work on this PR" })).toBeEnabled();
});

test("A stack's PR checks out the whole stack, and every row of it shows the one worktree", async ({ page }) => {
  await openInbox(page);

  const row = page.getByRole("listitem", { name: /^#403/ });
  await expect(row.getByRole("button", { name: "Start agent review" })).toBeDisabled();
  await row.getByRole("button", { name: "More actions" }).click();
  await page.getByRole("menuitem", { name: "Check out #403" }).click();

  const stack = page.getByRole("region", { name: /^Stack focus-rings/ });
  await expect(stack.getByRole("button", { name: "Check out stack" })).toHaveCount(0);
  await expect(stack.getByRole("img", { name: /Worktree jo-r-focus-trap/ }).first()).toBeVisible();
  await expect(row.locator("[data-worktree-state='here']")).toBeVisible();
  await expect(
    page.getByRole("listitem", { name: /^#401/ }).locator("[data-worktree-state='elsewhere']"),
  ).toBeVisible();
  await expect(row.getByRole("button", { name: "Start agent review" })).toBeEnabled();
  await page.screenshot({ path: test.info().outputPath("stack-checked-out.png") });
});

test("The Stack page switches its worktree to a PR through the desk, and refuses one with changes", async ({
  page,
}) => {
  await openInbox(page);
  await page.getByRole("button", { name: "Open stack driver-trait" }).click();

  const panel = page.getByLabel("Worktree harness-stack", { exact: true });
  await expect(panel).toContainText("/Users/ciaran/worktrees/codex-custom/harness-stack");
  await expect(panel).toContainText("On feat/claude-driver");
  const lower = page.getByRole("button", { name: /^#412/ });
  await expect(lower.locator("[data-worktree-state='elsewhere']")).toBeVisible();
  await page.screenshot({ path: test.info().outputPath("stack-page.png") });

  // Uncommitted changes in the worktree: the desk refuses to switch it.
  await setStackDirty(page, true);
  await lower.click({ button: "right" });
  await page.getByRole("menuitem", { name: /^Check out/ }).click();
  await expect(page.getByText(/has uncommitted changes/)).toBeVisible();
  await expect(page.getByRole("button", { name: /^#415/ }).locator("[data-worktree-state='here']")).toBeVisible();

  await setStackDirty(page, false);
  await lower.click({ button: "right" });
  await page.getByRole("menuitem", { name: /^Check out/ }).click();
  await expect(page.getByText("harness-stack is on feat/driver-trait")).toBeVisible();
  await expect(lower.locator("[data-worktree-state='here']")).toBeVisible();
});

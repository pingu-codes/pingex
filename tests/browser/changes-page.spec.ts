import { expect, type Page, test } from "@playwright/test";

// The preview thread works in a Stack worktree of `api` (#41 below #42, which
// is checked out with uncommitted edits) and on a git branch of `web`.

async function openChanges(page: Page) {
  await page.goto("/");
  await page.getByRole("button", { name: /Custom frontend skeleton/ }).click();
  await expect(page.getByRole("button", { name: "Worked for 24s" })).toBeVisible();
  await page.getByRole("button", { name: "Open Changes" }).click();
  await expect(page.getByRole("heading", { name: "Changes", exact: true })).toBeVisible();
}

function rail(page: Page) {
  return page.getByRole("navigation", { name: "Change segments" });
}

function diffs(page: Page) {
  return page.getByRole("region", { name: "Diffs" });
}

test("splits the thread's changes by repository and stack PR", async ({ page }) => {
  await openChanges(page);
  const segments = rail(page);
  await expect(segments.getByRole("region", { name: "api" })).toBeVisible();
  await expect(segments.getByRole("region", { name: "web" })).toBeVisible();
  await expect(segments.getByRole("button", { name: /#41 Ledger model/ })).toBeVisible();
  await expect(segments.getByRole("button", { name: /#42 Charge customers/ })).toBeVisible();

  // It opens on the working tree, untracked files included.
  await expect(segments.getByRole("button", { name: /Uncommitted/ })).toHaveAttribute("aria-current", "true");
  await expect(diffs(page).locator('[data-file="notes/plan.md"]')).toBeVisible();

  // Each PR shows against the one below it.
  await segments.getByRole("button", { name: /#41 Ledger model/ }).click();
  await expect(diffs(page).locator('[data-file="src/ledger/model.ts"]')).toBeVisible();
  await expect(diffs(page).getByText("compared with")).toContainText("origin/main");
  await segments.getByRole("button", { name: /#42 Charge customers/ }).click();
  await expect(diffs(page).locator('[data-file="src/billing/charge.test.ts"]')).toBeVisible();
  await expect(diffs(page).getByText("compared with")).toContainText("#41");

  // The other repository is one click away.
  await segments.getByRole("button", { name: /billing-ui/ }).click();
  await expect(diffs(page).locator('[data-file="src/routes/billing/+page.svelte"]')).toBeVisible();

  // "This thread" narrows the split.
  await page.getByRole("radio", { name: "This thread" }).check({ force: true });
  await expect(segments.getByRole("button", { name: /#41 Ledger model/ })).toHaveCount(0);
});

test("sends notes on lines back to the agent as one message", async ({ page }) => {
  await openChanges(page);
  const file = diffs(page).locator('[data-file="src/billing/charge.ts"]');
  await file.getByRole("button", { name: "Select line 2", exact: true }).first().click();
  await page.keyboard.press("c");
  await page.getByPlaceholder("Leave a comment").fill("Guard against a negative amount.");
  await page.getByRole("button", { name: "Add note" }).click();
  await expect(file.getByText("Guard against a negative amount.")).toBeVisible();

  await page.getByRole("button", { name: "Send 1 note to agent" }).click();
  await expect(page.getByRole("button", { name: "Send notes to agent" })).toBeDisabled();

  await page.getByRole("button", { name: "Thread", exact: true }).click();
  await expect(page.getByText("I left a note on your changes:").first()).toBeVisible();
  await expect(page.getByText("Guard against a negative amount.").first()).toBeVisible();
});

test("] and v walk the files as on the review page: the rail follows, and a viewed file folds", async ({ page }) => {
  // Wide enough for the file rail beside the segments.
  await page.setViewportSize({ width: 1500, height: 800 });
  await openChanges(page);
  const current = page.getByRole("navigation", { name: "Changed files" }).locator('[aria-current="true"]');
  await expect(current).toHaveText("charge.ts");

  await page.keyboard.press("]");
  await expect(current).toHaveText("plan.md");

  await page.keyboard.press("v");
  await expect(page.getByRole("checkbox", { name: "Viewed notes/plan.md" })).toBeChecked();
  await expect(page.getByRole("button", { name: "Toggle notes/plan.md" })).toHaveAttribute("aria-expanded", "false");
});

test("opens on the file chosen in the overview, and Back returns to the thread", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: /Custom frontend skeleton/ }).click();
  await expect(page.getByRole("button", { name: "Worked for 24s" })).toBeVisible();
  await page.getByRole("menu", { name: "Thread overview panel" }).getByTitle("src/ledger/model.ts").click();
  await expect(rail(page).getByRole("button", { name: /#41 Ledger model/ })).toHaveAttribute("aria-current", "true");
  await page.getByRole("button", { name: "Thread", exact: true }).click();
  await expect(page.getByRole("button", { name: "Worked for 24s" })).toBeVisible();
});

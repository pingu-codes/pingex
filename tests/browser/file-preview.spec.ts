import { expect, type Page, test } from "@playwright/test";

async function openPreviewThread(page: Page, { keepOverview = false } = {}) {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "New thread", exact: true })).toBeVisible();
  await page.getByRole("button", { name: /Custom frontend skeleton/ }).click();
  await expect(page.getByRole("button", { name: "Worked for 24s" })).toBeVisible();
  // The overview floats over the thread; close it as a user would, except when
  // the test needs the menu it holds.
  if (!keepOverview) await page.getByRole("button", { name: "Thread overview" }).click();
}

/** Click a path link the way agent output renders one. */
async function clickPathLink(page: Page, href: string) {
  await page.evaluate((target) => {
    const anchor = document.createElement("a");
    anchor.href = target;
    anchor.textContent = target;
    anchor.id = "spec-path-link";
    document.body.appendChild(anchor);
  }, href);
  await page.locator("#spec-path-link").click();
  await page.evaluate(() => document.getElementById("spec-path-link")?.remove());
}

const dialog = (page: Page) => page.getByRole("dialog");

test.beforeEach(async ({ page }) => {
  await openPreviewThread(page);
});

test("previews a cited file and scrolls to the line", async ({ page }) => {
  await clickPathLink(page, "/Users/me/project/src/lib/review/review.test.ts:9");

  await expect(dialog(page)).toBeVisible();
  await expect(dialog(page).locator('[data-part="title"]')).toHaveText("review.test.ts");
  // The subtitle carries the cited line.
  await expect(dialog(page).getByText(/review\.test\.ts:9/)).toBeVisible();

  // Gutter numbers every line of the fixture, and the code is highlighted.
  const gutter = dialog(page).locator('[data-testid="gutter"]');
  await expect(gutter).toContainText("11");
  await expect(dialog(page).locator("code.hljs .hljs-keyword").first()).toBeVisible();

  // The cited line is banded in both columns — the sticky gutter has an opaque
  // background, so it carries its own.
  await expect(dialog(page).locator('[data-testid="line-band"]')).toHaveCount(1);
  await expect(dialog(page).locator('[data-testid="gutter-band"]')).toHaveCount(1);
});

test("offers Finder, Zed and both copy forms", async ({ page }) => {
  await clickPathLink(page, "/Users/me/project/src/lib/a.ts:3");

  for (const name of ["Copy path", "Show in Finder", "Open in Zed"]) {
    await expect(dialog(page).getByRole("button", { name, exact: true })).toBeVisible();
  }
  // The cwd is unknown for a link like this, so there is no relative form.
  await expect(dialog(page).getByRole("button", { name: "Copy relative", exact: true })).toHaveCount(0);
  await dialog(page).getByLabel("Close").click();
  await expect(dialog(page)).toBeHidden();
});

test("explains a file it cannot preview, keeping the actions", async ({ page }) => {
  await clickPathLink(page, "/Users/me/project/assets/logo.png");

  await expect(dialog(page).getByTestId("preview-problem")).toContainText("binary");
  await expect(dialog(page).locator("code.hljs")).toHaveCount(0);
  await expect(dialog(page).getByRole("button", { name: "Show in Finder", exact: true })).toBeVisible();
  await expect(dialog(page).getByRole("button", { name: "Open in Zed", exact: true })).toBeVisible();
});

test("explains a missing file rather than doing nothing", async ({ page }) => {
  await clickPathLink(page, "/Users/me/project/missing/gone.ts:4");

  await expect(dialog(page).getByTestId("preview-problem")).toContainText("no longer exists");
});

test("opens a file picked from the right panel's file tree", async ({ page }) => {
  await openPreviewThread(page, { keepOverview: true });
  await page.getByRole("button", { name: "Files", exact: true }).click();
  await page.getByRole("button", { name: "README.md", exact: true }).click();

  await expect(dialog(page)).toBeVisible();
  await expect(dialog(page).locator('[data-part="title"]')).toHaveText("README.md");
  // Reached through the file tree, so there is no cited line and no band.
  await expect(dialog(page).locator('[data-testid="line-band"]')).toHaveCount(0);
});

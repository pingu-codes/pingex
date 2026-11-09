// The review page scrolls as one page: the topbar leaves with the code, the
// rails stay, and Your review folds to a strip that keeps its draft.

import { expect, type Page, test } from "@playwright/test";

async function openPr(page: Page) {
  await page.setViewportSize({ width: 1500, height: 800 });
  await page.goto("/?largeDiff=40x150");
  await page.getByRole("button", { name: "Pull requests" }).first().click();
  await page.getByText("Add pull-request review view").first().click();
  const diffs = page.getByRole("region", { name: "Diffs" });
  await expect(diffs.locator(".ln").first()).toBeVisible();
  return { diffs, pane: page.locator("[data-diff-pane]") };
}

test("the topbar scrolls away, and Back to top brings it back", async ({ page }) => {
  const { pane } = await openPr(page);
  const topbar = page.getByRole("button", { name: "Pull requests", exact: true }).last();
  const backToTop = page.getByRole("button", { name: "Back to top" });
  const yourReview = page.getByRole("complementary", { name: "Your review" });
  const tree = page.getByRole("navigation", { name: "Changed files" });

  await expect(topbar).toBeInViewport();
  await expect(backToTop).toBeHidden();

  await pane.evaluate((element) => (element.scrollTop = 3000));
  await expect(topbar).not.toBeInViewport();
  await expect(backToTop).toBeVisible();
  // The rails stay on screen, pinned to the top of the page.
  await expect(yourReview).toBeInViewport();
  await expect(tree).toBeInViewport();
  const paneTop = (await pane.boundingBox())!.y;
  expect((await tree.boundingBox())!.y).toBe(paneTop);

  await backToTop.click();
  expect(await pane.evaluate((element) => element.scrollTop)).toBe(0);
  await expect(topbar).toBeInViewport();
  await expect(backToTop).toBeHidden();
});

test("a partly scrolled topbar has no Back to top", async ({ page }) => {
  const { pane } = await openPr(page);
  await pane.evaluate((element) => (element.scrollTop = 10));
  await page.waitForTimeout(100);
  await expect(page.getByRole("button", { name: "Back to top" })).toBeHidden();
});

test("Your review collapses, keeps its draft, and stays collapsed on the next PR open", async ({ page }) => {
  const { diffs } = await openPr(page);
  const summary = page.getByRole("textbox", { name: "Summary" });
  const collapse = page.getByRole("button", { name: "Collapse Your review" });
  const expand = page.getByRole("button", { name: "Expand Your review" });
  await summary.fill("half written");
  const narrow = (await diffs.boundingBox())!.width;

  await collapse.click();
  await expect(summary).toBeHidden();
  await expect(expand).toBeVisible();
  expect((await diffs.boundingBox())!.width).toBeGreaterThan(narrow + 200);

  await expand.click();
  await expect(summary).toHaveValue("half written");
  await expect(expand).toBeHidden();

  await collapse.click();
  await page.reload();
  await page.getByRole("button", { name: "Pull requests" }).first().click();
  await page.getByText("Add pull-request review view").first().click();
  await expect(expand).toBeVisible();
  await expect(summary).toBeHidden();
});

test("the shortcuts card shows wherever the page is scrolled", async ({ page }) => {
  const { pane } = await openPr(page);
  await pane.evaluate((element) => (element.scrollTop = 3000));
  await expect(page.getByRole("button", { name: "Back to top" })).toBeVisible();
  await page.keyboard.press("?");
  await expect(page.getByRole("dialog", { name: "Shortcuts" })).toBeInViewport();
});

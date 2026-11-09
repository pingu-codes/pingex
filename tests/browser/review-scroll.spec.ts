// The review page scrolls as one page: the topbar leaves with the code, the
// rails stay, and Your review folds to a strip that keeps its draft.

import { expect, type Page, test } from "@playwright/test";

async function openPr(page: Page, shape = "40x150") {
  await page.setViewportSize({ width: 1500, height: 800 });
  await page.goto(`/?largeDiff=${shape}`);
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

test.describe("the file rail", () => {
  const rail = (page: Page) => page.getByRole("navigation", { name: "Changed files" });
  const highlighted = (page: Page) => rail(page).locator('[aria-current="true"]');
  /** The fixture pads its numbers. */
  const name = (index: number) => `file-${String(index).padStart(2, "0")}.ts`;
  const fileAt = (index: number) => `[data-file="src/large/${name(index)}"]`;

  /** Put the page `into` pixels past the top of a file. */
  async function scrollInto(page: Page, index: number, into: number) {
    await page.locator(fileAt(index)).evaluate((element, into) => {
      const pane = element.closest("[data-diff-pane]")!;
      pane.scrollTop += element.getBoundingClientRect().top - pane.getBoundingClientRect().top + into;
    }, into);
  }

  /** How far a file's top sits from the top of the page's view. */
  const offset = (page: Page, index: number) =>
    page.locator(fileAt(index)).evaluate((element) => {
      const pane = element.closest("[data-diff-pane]")!;
      return Math.round(element.getBoundingClientRect().top - pane.getBoundingClientRect().top);
    });

  test("the highlight follows the scroll, and the rail keeps it in view", async ({ page }) => {
    await openPr(page);
    await expect(highlighted(page)).toHaveText(name(0));

    await scrollInto(page, 30, 500);
    await expect(highlighted(page)).toHaveText(name(30));
    await expect.poll(() => rail(page).evaluate((nav) => nav.scrollTop)).toBeGreaterThan(0);
    const box = (await rail(page).boundingBox())!;
    const row = (await highlighted(page).boundingBox())!;
    expect(row.y).toBeGreaterThanOrEqual(box.y);
    expect(row.y + row.height).toBeLessThanOrEqual(box.y + box.height);

    await scrollInto(page, 2, 500);
    await expect(highlighted(page)).toHaveText(name(2));
    await expect(highlighted(page)).toBeInViewport();
    await expect.poll(() => rail(page).evaluate((nav) => nav.scrollTop)).toBeLessThan(100);
  });

  test("v walks down the files, each staying in place as it collapses", async ({ page }) => {
    await openPr(page);
    await scrollInto(page, 3, 1000);
    await expect(highlighted(page)).toHaveText(name(3));

    await page.keyboard.press("v");
    await expect(page.getByRole("checkbox", { name: `Viewed src/large/${name(3)}` })).toBeChecked();
    // Its header comes back to the top, rather than the next files going past.
    await expect.poll(() => offset(page, 3)).toBe(0);
    await expect(highlighted(page)).toHaveText(name(4));

    await page.keyboard.press("v");
    await expect(page.getByRole("checkbox", { name: `Viewed src/large/${name(4)}` })).toBeChecked();
    await expect(highlighted(page)).toHaveText(name(5));
    await expect.poll(() => offset(page, 3)).toBe(0);
  });

  test("collapsing the files above keeps the file being read still", async ({ page }) => {
    await openPr(page);
    await scrollInto(page, 7, -300);
    await expect(highlighted(page)).toHaveText(name(6));

    // Alt-click collapses every other file.
    await page.getByRole("button", { name: `Toggle src/large/${name(7)}` }).click({ modifiers: ["Alt"] });

    await expect(highlighted(page)).toHaveText(name(7));
    await expect.poll(() => offset(page, 7)).toBe(300);
  });

  test("collapsing the file at the top moves the highlight to the next", async ({ page }) => {
    await openPr(page);
    await rail(page)
      .getByRole("button", { name: name(2) })
      .click();
    await expect(highlighted(page)).toHaveText(name(2));
    await page.getByRole("button", { name: `Toggle src/large/${name(2)}` }).click();
    await expect(highlighted(page)).toHaveText(name(3));
  });

  test("short files at the end of the page still get the highlight", async ({ page }) => {
    await openPr(page, "40x3");
    const pane = page.locator("[data-diff-pane]");

    // A file that can't reach the top of the view keeps a click's highlight.
    await rail(page)
      .getByRole("button", { name: name(37) })
      .click();
    await page.waitForTimeout(100);
    await expect(highlighted(page)).toHaveText(name(37));

    // Scrolling up lets go of it, and scrolling back down reaches the last file.
    await pane.evaluate((element) => (element.scrollTop -= 1000));
    await expect(highlighted(page)).not.toHaveText(name(37));
    await pane.evaluate((element) => (element.scrollTop = element.scrollHeight));
    await expect(highlighted(page)).toHaveText(name(39));

    await pane.evaluate((element) => (element.scrollTop -= 400));
    await expect(highlighted(page)).not.toHaveText(name(39));
  });

  test("a long folder name wraps instead of being cut off", async ({ page }) => {
    await openPr(page);
    const folder = rail(page).locator(".eyebrow").first();
    const short = (await folder.boundingBox())!.height;
    await folder.evaluate((element) => {
      element.textContent =
        "packages/some-long-package-name/src/components/deeply/nested/averyveryveryveryverylongfoldername";
    });
    expect((await folder.boundingBox())!.height).toBeGreaterThan(short);
    const fits = (element: Element) => element.scrollWidth <= element.clientWidth;
    expect(await folder.evaluate(fits)).toBe(true);
    expect(await rail(page).evaluate(fits)).toBe(true);
  });
});

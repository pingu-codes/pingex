// Cmd+F on the review page, where it has to work against real paint, layout and
// row mounting: matches in rows not yet in the DOM, the Highlight API across
// highlighted tokens, and the file's sideways scroll. See `FindBar.svelte`.

import { expect, type Locator, type Page, test } from "@playwright/test";

const MANY_FILES = "250x120";
const ONE_HUGE_FILE = "1x5000";

async function openLargeDiff(page: Page, shape: string): Promise<Locator> {
  await page.goto(`/?largeDiff=${shape}`);
  await page.getByRole("button", { name: "Pull requests" }).first().click();
  await page.getByText("Add pull-request review view").first().click();
  const diffs = page.getByRole("region", { name: "Diffs" });
  await expect(diffs.locator(".ln").first()).toBeVisible();
  return diffs;
}

async function find(page: Page, query: string) {
  await page.keyboard.press("ControlOrMeta+f");
  const input = page.getByRole("textbox", { name: "Find on this tab" });
  await expect(input).toBeFocused();
  await input.fill(query);
}

/** The text each of the page's two highlights marks. A static range whose row
 *  has gone reads as "". */
function highlighted(page: Page) {
  return page.evaluate(() => {
    const read = (name: string) =>
      [...((CSS.highlights.get(name) as unknown as Iterable<AbstractRange>) ?? [])].map((item) => {
        if (!item.startContainer.isConnected) return "";
        const range = document.createRange();
        range.setStart(item.startContainer, item.startOffset);
        range.setEnd(item.endContainer, item.endOffset);
        return range.toString();
      });
    return { matches: read("review-find"), current: read("review-find-current") };
  });
}

const paneTop = (diffs: Locator) =>
  diffs.locator("xpath=ancestor::*[@data-diff-pane]").evaluate((element) => element.scrollTop);

test.describe("find on the review page", () => {
  test("reaches a match in a far file whose rows are not mounted", async ({ page }) => {
    const diffs = await openLargeDiff(page, MANY_FILES);
    const last = diffs.locator('[data-file="src/large/file-249.ts"]');
    await expect(last.locator(".ln")).toHaveCount(0);

    await find(page, 'compute(7, "249")');
    await expect(page.getByText("1 of 1", { exact: true })).toBeVisible();

    const row = last.locator(".ln", { hasText: 'compute(7, "249")' });
    await expect(row).toBeInViewport();
    await expect.poll(async () => (await highlighted(page)).current).toEqual(['compute(7, "249")']);
  });

  test("paints a match across highlighted tokens, through the highlight landing", async ({ page }) => {
    await openLargeDiff(page, MANY_FILES);
    // `const`, the name and `=` are three tokens once highlighted.
    await find(page, "const value3 =");
    await expect(page.getByText(/^1 of \d/)).toBeVisible();
    await expect.poll(async () => (await highlighted(page)).current).toEqual(["const value3 ="]);
    // The row has its tokens now, and the Range is still on live text.
    const row = page.locator(".ln", { hasText: "const value3 =" }).first();
    await expect(row.locator('[class^="hljs-"]').first()).toBeAttached();
    expect((await highlighted(page)).current).toEqual(["const value3 ="]);
  });

  test("ranges follow rows as they unmount and mount again", async ({ page }) => {
    const diffs = await openLargeDiff(page, MANY_FILES);
    await find(page, 'compute(0, "0")');
    await expect(page.getByText("1 of 1", { exact: true })).toBeVisible();
    await expect.poll(async () => (await highlighted(page)).current).toHaveLength(1);

    // A common query, past the cap, paints only what is mounted.
    await page.getByRole("textbox", { name: "Find on this tab" }).fill("compute(");
    await expect(page.getByText(/^\d[\d,]* of 10,000\+$/)).toBeVisible();
    await expect.poll(async () => (await highlighted(page)).matches.length).toBeGreaterThan(0);
    const mounted = await diffs.locator(".ln").count();
    const painted = (await highlighted(page)).matches.length;
    expect(painted).toBeLessThanOrEqual(mounted);

    const pane = diffs.locator("xpath=ancestor::*[@data-diff-pane]");
    await pane.evaluate((element) => (element.scrollTop = element.scrollHeight / 2));
    // The current match's chunk stays mounted, and the rest of its neighbours go.
    await expect(diffs.locator('[data-file="src/large/file-01.ts"] .ln')).toHaveCount(0);
    // Painted again for the rows mounted here, each on live text.
    await expect
      .poll(async () => {
        const { matches } = await highlighted(page);
        return matches.length > 0 && matches.every((text) => text === "compute(");
      })
      .toBe(true);
  });

  test("a match far along a long line scrolls the file sideways, and back", async ({ page }) => {
    const diffs = await openLargeDiff(page, ONE_HUGE_FILE);
    const body = diffs.locator("[data-file] > .overflow-x-auto");
    const scrollLeft = () => body.evaluate((element) => element.scrollLeft);
    /** Whether the current match is clear of the line numbers and the file's edge. */
    const currentInView = () =>
      body.evaluate((element) => {
        const [range] = CSS.highlights.get("review-find-current") as unknown as Iterable<Range>;
        const match = range.getBoundingClientRect();
        const box = element.getBoundingClientRect();
        return match.left >= box.left + 104 && match.right <= box.right;
      });
    // The fixture pads each long line's comment with dashes to 240 columns, so
    // this query matches a long line seven times, the last far off the edge.
    await find(page, "-".repeat(30));
    await expect(page.getByText(/^1 of \d/)).toBeVisible();
    await expect.poll(currentInView).toBe(true);
    const nearStart = await scrollLeft();

    for (let press = 0; press < 6; press++) await page.keyboard.press("Enter");
    await expect(page.getByText(/^7 of \d/)).toBeVisible();
    await expect.poll(scrollLeft).toBeGreaterThan(nearStart);
    await expect.poll(currentInView).toBe(true);

    for (let press = 0; press < 6; press++) await page.keyboard.press("Shift+Enter");
    await expect(page.getByText(/^1 of \d/)).toBeVisible();
    await expect.poll(currentInView).toBe(true);
  });

  test("the find bar stays put as the page scrolls", async ({ page }) => {
    const diffs = await openLargeDiff(page, MANY_FILES);
    await find(page, 'compute(100, "200")');
    await expect(page.getByText("1 of 1", { exact: true })).toBeVisible();
    await expect.poll(() => paneTop(diffs)).toBeGreaterThan(1000);
    await expect(page.getByRole("textbox", { name: "Find on this tab" })).toBeInViewport();
  });

  test("closing clears the page's highlights", async ({ page }) => {
    await openLargeDiff(page, MANY_FILES);
    await find(page, "const value1 ");
    await expect.poll(async () => (await highlighted(page)).current).toHaveLength(1);
    await page.keyboard.press("Escape");
    expect(await highlighted(page)).toEqual({ matches: [], current: [] });
  });

  test("the conversation tab's matches paint too", async ({ page }) => {
    await openLargeDiff(page, "2x10");
    await page.getByRole("radio", { name: /^Conversation/ }).check({ force: true });
    await find(page, "looks solid. One general");
    await expect(page.getByText("1 of 1", { exact: true })).toBeVisible();
    await expect.poll(async () => (await highlighted(page)).current).toEqual(["looks solid. One general"]);
  });
});

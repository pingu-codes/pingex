// The review page's Files tab on a large diff (`?largeDiff=<files>x<lines>`,
// see `deskPrDetail`): how much DOM each diff line costs, that off-screen
// files skip rendering, and, in WebKit (the engine the macOS app ships),
// how smoothly it scrolls.

import { expect, type Locator, type Page, test } from "@playwright/test";

const MANY_FILES = "40x150";
const ONE_HUGE_FILE = "1x5000";

async function openLargeDiff(page: Page, shape: string): Promise<Locator> {
  await page.goto(`/?largeDiff=${shape}`);
  await page.getByRole("button", { name: "Pull requests" }).first().click();
  await page.getByText("Add pull-request review view").first().click();
  const diffs = page.getByRole("region", { name: "Diffs" });
  await expect(diffs.locator(".ln").first()).toBeVisible();
  return diffs;
}

test.describe("render budget", () => {
  test("a diff line costs a handful of DOM nodes and no SVG", async ({ page }) => {
    const diffs = await openLargeDiff(page, MANY_FILES);
    const { lines, nodes, svgs } = await diffs.evaluate((section) => {
      const body = section.querySelector("[data-file] .ln")!.parentElement!;
      const rows = body.querySelectorAll(".ln");
      return {
        lines: rows.length,
        nodes: body.querySelectorAll("*").length,
        svgs: body.querySelectorAll(".ln svg").length,
      };
    });
    expect(lines).toBeGreaterThan(100);
    expect(svgs).toBe(0);
    // A row, two line numbers, the marker and the code.
    expect(nodes / lines).toBeLessThanOrEqual(6);
  });

  test("files out of view skip rendering but keep their height", async ({ page }) => {
    const diffs = await openLargeDiff(page, MANY_FILES);
    const { visibility, lines, scrollHeight } = await diffs.evaluate((section) => {
      const bodies = [...section.querySelectorAll("[data-file]")].map(
        (file) => file.querySelector(".ln")!.parentElement!,
      );
      return {
        visibility: getComputedStyle(bodies.at(-1)!).getPropertyValue("content-visibility"),
        lines: section.querySelectorAll(".ln").length,
        scrollHeight: section.scrollHeight,
      };
    });
    expect(visibility).toBe("auto");
    // Skipped files hold their estimated height, so the scrollbar is honest.
    expect(scrollHeight).toBeGreaterThan(lines * 20 * 0.95);
  });
});

/** Scroll the Diffs pane `pixelsPerFrame` per animation frame for `frames`
 *  frames, and return each frame's duration in milliseconds. */
async function scrollFrames(diffs: Locator, pixelsPerFrame: number, frames: number): Promise<number[]> {
  return diffs.evaluate(
    (section, { pixelsPerFrame, frames }) =>
      new Promise<number[]>((resolve) => {
        const deltas: number[] = [];
        let last = performance.now();
        const step = (now: number) => {
          deltas.push(now - last);
          last = now;
          section.scrollTop += pixelsPerFrame;
          if (deltas.length < frames) requestAnimationFrame(step);
          else resolve(deltas.slice(1));
        };
        requestAnimationFrame(step);
      }),
    { pixelsPerFrame, frames },
  );
}

function summarize(deltas: number[]) {
  const sorted = [...deltas].sort((a, b) => a - b);
  const at = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))];
  return { p50: at(0.5), p95: at(0.95), long: deltas.filter((delta) => delta > 32).length, frames: deltas.length };
}

test.describe("scroll benchmark", () => {
  test.skip(({ browserName }) => browserName !== "webkit", "WebKit is what the macOS app renders with");
  test.describe.configure({ timeout: 120_000 });

  // After the fix both shapes run at p95 ≈ 26ms with no frame over 32ms;
  // before it, 40x150 ran at p95 300ms and 1x5000 at p95 45-60ms with 11-39
  // long frames.
  for (const shape of [MANY_FILES, ONE_HUGE_FILE]) {
    test(`scrolling ${shape} stays smooth`, async ({ page }) => {
      const diffs = await openLargeDiff(page, shape);
      // Warm up: the first frames pay for initial layout.
      await scrollFrames(diffs, 0, 10);
      const result = summarize(await scrollFrames(diffs, 80, 180));
      const line = `${shape}: p50 ${result.p50.toFixed(1)}ms, p95 ${result.p95.toFixed(1)}ms, ${result.long}/${result.frames} frames over 32ms`;
      console.log(line);
      test.info().annotations.push({ type: "scroll", description: line });
      expect(result.p95).toBeLessThan(40);
      expect(result.long).toBeLessThanOrEqual(result.frames * 0.05);
    });
  }
});

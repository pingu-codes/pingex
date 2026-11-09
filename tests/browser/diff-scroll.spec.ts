// The review page's Files tab on a large diff (`?largeDiff=<files>x<lines>`,
// see `deskPrDetail`): how much DOM each diff line costs, that rows far off
// screen are not mounted, and, in WebKit (the engine the macOS app ships),
// how smoothly it opens, scrolls and answers clicks.

import { expect, type Locator, type Page, test } from "@playwright/test";

const SOME_FILES = "40x150";
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

test.describe("render budget", () => {
  test("a diff line costs a handful of DOM nodes and no SVG", async ({ page }) => {
    const diffs = await openLargeDiff(page, SOME_FILES);
    // Highlighting may land a frame after the rows.
    await expect(diffs.locator('[data-file] .ln [class^="hljs-"]').first()).toBeAttached();
    const { lines, nodes, tokens, svgs } = await diffs.evaluate((section) => {
      const body = section.querySelector("[data-file]")!;
      const rows = body.querySelectorAll(".ln");
      const tokens = body.querySelectorAll('.ln [class^="hljs-"]').length;
      return {
        lines: rows.length,
        nodes: [...rows].reduce((sum, row) => sum + 1 + row.querySelectorAll("*").length, 0) - tokens,
        tokens,
        svgs: body.querySelectorAll(".ln svg").length,
      };
    });
    expect(lines).toBeGreaterThan(100);
    expect(svgs).toBe(0);
    // A row, two line numbers, the marker and the code.
    expect(nodes / lines).toBeLessThanOrEqual(6);
    // The fixture is TypeScript, so its code is highlighted: a span per token.
    expect(tokens).toBeGreaterThan(0);
    expect(tokens / lines).toBeLessThanOrEqual(10);
  });

  test("rows far off screen are not mounted, and the scrollbar stays honest", async ({ page }) => {
    const diffs = await openLargeDiff(page, MANY_FILES);
    const { mounted, scrollHeight } = await diffs.evaluate((section) => ({
      mounted: section.querySelectorAll(".ln").length,
      scrollHeight: section.closest("[data-diff-pane]")!.scrollHeight,
    }));
    // 250 files of 120 lines in three hunks: each line and hunk header a row.
    const total = 250 * (120 + 3);
    // A few viewports of rows, not the whole PR.
    expect(mounted).toBeLessThan(600);
    // Unmounted rows hold their 20px each, so the scrollbar is honest.
    expect(scrollHeight).toBeGreaterThan(total * 20);
  });
});

test.describe("mounting rows near the viewport", () => {
  test("a far file jumped to shows its rows, highlighted", async ({ page }) => {
    const diffs = await openLargeDiff(page, MANY_FILES);
    // Its rows are not mounted yet.
    const last = diffs.locator('[data-file="src/large/file-249.ts"]');
    await expect(last.locator(".ln")).toHaveCount(0);
    // As the file tree's jump does (the tree is hidden at this width). The
    // last file is where a layout forced mid-update would clamp the scroll:
    // the header stays at the top while the rows around it mount.
    const offsets = await last.evaluate(
      (element) =>
        new Promise<number[]>((resolve) => {
          const pane = element.closest("[data-diff-pane]")!;
          const offset = () => Math.round(element.getBoundingClientRect().top - pane.getBoundingClientRect().top);
          element.scrollIntoView({ block: "start" });
          const seen: number[] = [];
          const step = () => {
            seen.push(offset());
            if (seen.length < 10) requestAnimationFrame(step);
            else resolve(seen);
          };
          requestAnimationFrame(step);
        }),
    );
    expect(offsets).toEqual(Array(10).fill(offsets[0]));
    await expect(last.locator(".ln").first()).toBeInViewport();
    await expect(last.locator('.ln [class^="hljs-"]').first()).toBeAttached();
    // The first file's rows are far away now, and unmounted.
    await expect(diffs.locator('[data-file="src/large/file-00.ts"] .ln')).toHaveCount(0);
  });

  test("a file's menu opens from its header", async ({ page }) => {
    const diffs = await openLargeDiff(page, SOME_FILES);
    const head = diffs.locator('[data-file="src/large/file-00.ts"] .file-head');
    await head.hover();
    await head.getByRole("button", { name: "File actions" }).click();
    await expect(page.getByRole("menuitem", { name: /Mark viewed/ })).toBeVisible();
  });

  test("a comment draft survives scrolling far away and back", async ({ page }) => {
    const diffs = await openLargeDiff(page, MANY_FILES);
    const first = diffs.locator('[data-file="src/large/file-00.ts"]');
    await first.getByRole("button", { name: "Select line 3", exact: true }).click();
    await page.keyboard.press("c");
    await page.getByPlaceholder("Leave a comment").fill("keep me");
    const pane = diffs.locator("xpath=ancestor::*[@data-diff-pane]");
    await pane.evaluate((element) => (element.scrollTop = element.scrollHeight / 2));
    await expect(diffs.locator('[data-file="src/large/file-01.ts"] .ln')).toHaveCount(0);
    await pane.evaluate((element) => (element.scrollTop = 0));
    await expect(page.getByPlaceholder("Leave a comment")).toHaveValue("keep me");
  });

  test("a file's horizontal scroll holds as its rows unmount", async ({ page }) => {
    const diffs = await openLargeDiff(page, ONE_HUGE_FILE);
    const body = diffs.locator("[data-file] > .overflow-x-auto");
    const width = await body.evaluate((element) => {
      element.scrollLeft = 1000;
      return element.scrollLeft;
    });
    expect(width).toBeGreaterThan(500);
    // Far down the file, the long lines near the top have unmounted.
    await diffs
      .locator("xpath=ancestor::*[@data-diff-pane]")
      .evaluate((element) => (element.scrollTop = element.scrollHeight - element.clientHeight));
    await expect(diffs.locator('.ln[data-line="1"]')).toHaveCount(0);
    expect(await body.evaluate((element) => element.scrollLeft)).toBe(width);
  });
});

/** Scroll the Diffs pane `pixelsPerFrame` per animation frame for `frames`
 *  frames, and return each frame's duration in milliseconds. */
async function scrollFrames(diffs: Locator, pixelsPerFrame: number, frames: number): Promise<number[]> {
  return diffs.evaluate(
    (section, { pixelsPerFrame, frames }) =>
      new Promise<number[]>((resolve) => {
        const pane = section.closest("[data-diff-pane]")!;
        const deltas: number[] = [];
        let last = performance.now();
        const step = (now: number) => {
          deltas.push(now - last);
          last = now;
          pane.scrollTop += pixelsPerFrame;
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

function report(line: string) {
  console.log(line);
  test.info().annotations.push({ type: "perf", description: line });
}

/** Milliseconds from running `act` in the page to the end of the frame that
 *  shows its result (the second animation frame after it). */
async function timeToFrame(page: Page, act: string): Promise<number> {
  return page.evaluate(
    (act) =>
      new Promise<number>((resolve) => {
        const start = performance.now();
        new Function(act)();
        requestAnimationFrame(() => requestAnimationFrame(() => resolve(performance.now() - start)));
      }),
    act,
  );
}

test.describe("perf benchmark", () => {
  test.skip(({ browserName }) => browserName !== "webkit", "WebKit is what the macOS app renders with");
  test.describe.configure({ timeout: 120_000 });

  // Before rows mounted in chunks (c6d4dac), WebKit, dev build:
  //   open 250x120: one 30-35s frame (68s to settle)
  //   scroll p95: 40x150 26ms, 250x120 33ms (10 long frames), 1x5000 26ms
  //   clicks 250x120: select 287ms, viewed 259ms, unviewed 150ms
  // After: open ~150ms longest frame, scroll p95 25-27ms with 0-1 long
  // frames on every shape, clicks 12-28ms.
  for (const shape of [SOME_FILES, MANY_FILES, ONE_HUGE_FILE]) {
    test(`scrolling ${shape} stays smooth, hovering too`, async ({ page }) => {
      const diffs = await openLargeDiff(page, shape);
      const box = (await diffs.boundingBox())!;
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
      // Warm up: the first frames pay for initial layout.
      await scrollFrames(diffs, 0, 10);
      const result = summarize(await scrollFrames(diffs, 80, 180));
      report(
        `scroll ${shape}: p50 ${result.p50.toFixed(1)}ms, p95 ${result.p95.toFixed(1)}ms, ${result.long}/${result.frames} frames over 32ms`,
      );
      expect(result.p95).toBeLessThan(40);
      expect(result.long).toBeLessThanOrEqual(result.frames * 0.05);
    });
  }

  // Every chunk that mounts or unmounts repaints the page's matches.
  test("scrolling stays smooth with a common find query open", async ({ page }) => {
    const diffs = await openLargeDiff(page, MANY_FILES);
    await page.keyboard.press("ControlOrMeta+f");
    await page.getByRole("textbox", { name: "Find on this tab" }).fill("value");
    await expect(page.getByText(/^1 of /)).toBeVisible();
    await scrollFrames(diffs, 0, 10);
    const result = summarize(await scrollFrames(diffs, 80, 180));
    report(
      `scroll with find ${MANY_FILES}: p50 ${result.p50.toFixed(1)}ms, p95 ${result.p95.toFixed(1)}ms, ${result.long}/${result.frames} frames over 32ms`,
    );
    expect(result.p95).toBeLessThan(40);
    expect(result.long).toBeLessThanOrEqual(result.frames * 0.05);
  });

  test("a search of a many-file PR answers within a few frames", async ({ page }) => {
    await openLargeDiff(page, MANY_FILES);
    await page.keyboard.press("ControlOrMeta+f");
    const input = page.getByRole("textbox", { name: "Find on this tab" });
    await input.fill("value");
    await expect(page.getByText(/^1 of /)).toBeVisible();
    // A word that stays unmatched: every one of the 30,000 lines gets searched.
    const elapsed = await page.evaluate(
      () =>
        new Promise<number>((resolve) => {
          const label = document.querySelector("[role=search] [aria-live]")!;
          const box = document.querySelector<HTMLInputElement>("[role=search] input")!;
          const observer = new MutationObserver(() => {
            if (!label.textContent?.includes("No results")) return;
            observer.disconnect();
            resolve(performance.now() - start);
          });
          observer.observe(label, { childList: true, characterData: true, subtree: true });
          const start = performance.now();
          box.value = "nothing matches this";
          box.dispatchEvent(new Event("input", { bubbles: true }));
        }),
    );
    // The debounce, then the search itself.
    report(`search ${MANY_FILES}: ${elapsed.toFixed(0)}ms including the 120ms debounce`);
    expect(elapsed).toBeLessThan(120 + 100);
  });

  test("opening a many-file PR never blocks for long", async ({ page }) => {
    await page.goto(`/?largeDiff=${MANY_FILES}`);
    await page.getByRole("button", { name: "Pull requests" }).first().click();
    // Watch frame gaps from just before the click until the rows show.
    await page.evaluate(() => {
      const state = { start: performance.now(), longest: 0, last: performance.now(), stop: false };
      (window as unknown as { __frames: typeof state }).__frames = state;
      const step = (now: number) => {
        state.longest = Math.max(state.longest, now - state.last);
        state.last = now;
        if (!state.stop) requestAnimationFrame(step);
      };
      requestAnimationFrame(step);
    });
    await page.getByText("Add pull-request review view").first().click();
    const diffs = page.getByRole("region", { name: "Diffs" });
    await expect(diffs.locator(".ln").first()).toBeVisible();
    // A second to settle: highlighting and anything else the open set off.
    await page.waitForTimeout(1000);
    const { total, longest } = await page.evaluate(() => {
      const state = (window as unknown as { __frames: { start: number; longest: number; stop: boolean } }).__frames;
      state.stop = true;
      return { total: performance.now() - state.start, longest: state.longest };
    });
    report(`open ${MANY_FILES}: longest frame ${longest.toFixed(0)}ms (${total.toFixed(0)}ms incl. 1s settle)`);
    expect(longest).toBeLessThan(250);
  });

  test("clicks answer within a frame or two on a many-file PR", async ({ page }) => {
    const diffs = await openLargeDiff(page, MANY_FILES);
    await page.waitForTimeout(500);
    const select = await timeToFrame(
      page,
      `const button = document.querySelector('[data-file] .ln[data-line] button');
       button.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 }));
       window.dispatchEvent(new PointerEvent('pointerup'));`,
    );
    await expect(diffs.locator(".ln-sel").first()).toBeVisible();
    const viewed = await timeToFrame(page, `document.querySelectorAll('[data-file] input[type=checkbox]')[1].click();`);
    const unviewed = await timeToFrame(
      page,
      `document.querySelectorAll('[data-file] input[type=checkbox]')[1].click();`,
    );
    report(
      `clicks ${MANY_FILES}: select ${select.toFixed(0)}ms, viewed ${viewed.toFixed(0)}ms, unviewed ${unviewed.toFixed(0)}ms`,
    );
    expect(select).toBeLessThan(50);
    expect(viewed).toBeLessThan(50);
    expect(unviewed).toBeLessThan(50);
  });
});

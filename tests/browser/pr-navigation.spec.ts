// Back retraces the way to a PR and each page comes back as it was left, and
// a PR in a stack opens with the stack beside it, which folds to a strip.

import { expect, type Page, test } from "@playwright/test";

const inboxRegion = (page: Page) => page.getByRole("region", { name: "Pull requests" });

async function openInbox(page: Page) {
  await page.setViewportSize({ width: 1500, height: 500 });
  await page.goto("/");
  await page.getByRole("button", { name: "Pull requests" }).first().click();
  await expect(inboxRegion(page).getByRole("radio", { name: /Everyone/ })).toBeAttached();
}

const showEveryone = (page: Page) => inboxRegion(page).locator("label", { hasText: "Everyone" }).click();

test("Back from a PR returns to the inbox with its filter and scroll", async ({ page }) => {
  await openInbox(page);
  await showEveryone(page);
  // Until the scroll event lands, a frame later: the page records its place from it.
  await inboxRegion(page).evaluate(
    (element) =>
      new Promise((resolve) => {
        element.addEventListener("scroll", resolve, { once: true });
        element.scrollTop = 200;
      }),
  );

  // Dispatched: a real click would first scroll the row into view, which moves the place.
  await inboxRegion(page).getByText("Add pull-request review view").first().dispatchEvent("click");
  await expect(page.getByRole("heading", { name: "Add pull-request review view" })).toBeVisible();
  await page.getByRole("button", { name: "Pull requests", exact: true }).last().click();

  await expect(inboxRegion(page).getByRole("radio", { name: /Everyone/ })).toBeChecked();
  // Within a few pixels: the header's late content grows the page, and scroll anchoring follows it.
  await expect
    .poll(async () => Math.abs((await inboxRegion(page).evaluate((element) => element.scrollTop)) - 200))
    .toBeLessThan(8);
});

test("a PR comes back at the place in its diff it was left", async ({ page }) => {
  await page.setViewportSize({ width: 1500, height: 800 });
  await page.goto("/?largeDiff=40x150");
  await page.getByRole("button", { name: "Pull requests" }).first().click();
  await page.getByText("Add pull-request review view").first().click();
  const pane = page.locator("[data-diff-pane]");
  await expect(page.getByRole("region", { name: "Diffs" }).locator(".ln").first()).toBeVisible();

  await pane.evaluate((element) => (element.scrollTop = 40000));
  await expect.poll(() => pane.evaluate((element) => element.scrollTop)).toBe(40000);
  const topFile = () =>
    pane.evaluate((element) => {
      const top = element.getBoundingClientRect().top;
      return [...element.querySelectorAll("[data-file]")]
        .filter((file) => file.getBoundingClientRect().top <= top)
        .at(-1)
        ?.getAttribute("data-file");
    });
  const fileBefore = await topFile();
  expect(fileBefore).toBeTruthy();

  // The topbar has scrolled away, and its Back with it: this one stays.
  await page.getByRole("button", { name: "Back to Pull requests" }).click();
  await expect(inboxRegion(page)).toBeVisible();
  await page.getByText("Add pull-request review view").first().click();

  await expect.poll(() => pane.evaluate((element) => element.scrollTop)).toBe(40000);
  expect(await topFile()).toBe(fileBefore);
  await expect(pane.locator(".ln:visible").first()).toBeVisible();
});

test("a PR in a stack opens with the stack, which folds and unfolds and stays folded", async ({ page }) => {
  await openInbox(page);
  await showEveryone(page);
  await inboxRegion(page).getByText("Harness driver trait").first().click();

  const tree = page.getByRole("region", { name: "Stack tree" });
  await expect(tree).toBeVisible();
  await expect(tree.getByRole("button", { name: /^#412 / })).toHaveAttribute("aria-pressed", "true");

  await page.getByRole("button", { name: "Collapse stack" }).click();
  await expect(tree).toBeHidden();
  const strip = page.getByRole("navigation", { name: "Stack branches" });
  await expect(strip.getByRole("button", { name: "#412" })).toHaveAttribute("aria-current", "true");

  // The PR takes the room the tree left.
  const pr = page.getByRole("region", { name: "Pull request 412" });
  await expect.poll(async () => (await pr.boundingBox())?.width ?? 0).toBeGreaterThan(900);

  await strip.getByRole("button", { name: "#415" }).click();
  await expect(page.getByRole("region", { name: "Pull request 415" })).toBeVisible();

  await page.getByRole("button", { name: "Pull requests", exact: true }).last().click();
  await expect(inboxRegion(page).getByRole("radio", { name: /Everyone/ })).toBeChecked();
  await inboxRegion(page).getByText("Claude Code driver over stream-json").first().click();
  await expect(strip).toBeVisible();

  await page.getByRole("button", { name: "Expand stack" }).click();
  await expect(tree).toBeVisible();
});

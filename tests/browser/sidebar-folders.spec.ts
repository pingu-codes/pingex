import { expect, type Locator, type Page, test } from "@playwright/test";

async function loadPreview(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "New thread", exact: true })).toBeVisible();
}

/** Drag one sidebar row onto another via pointer events (the sidebar doesn't
 *  use native HTML5 drag-and-drop, so Playwright's DnD helpers don't apply). */
async function dragRow(page: Page, from: Locator, to: Locator) {
  const src = await from.boundingBox();
  const dst = await to.boundingBox();
  if (!src || !dst) throw new Error("missing bounding box for drag");
  await page.mouse.move(src.x + src.width / 2, src.y + src.height / 2);
  await page.mouse.down();
  // Land on the middle third of the target row's height so `resolveDrop`
  // treats it as an "inside" drop for a folder target.
  await page.mouse.move(dst.x + dst.width / 2, dst.y + dst.height / 2, { steps: 10 });
  await page.mouse.up();
}

async function createFolder(page: Page, name: string) {
  await page.getByRole("button", { name: "New folder" }).click();
  await page.getByPlaceholder("Folder name").fill(name);
  await page.getByRole("button", { name: "Create" }).click();
}

function projectRow(page: Page, path: string) {
  return page.locator(`[data-sidebar-row="item:${path}"]`);
}

/** The folder's header row — what a drag actually lands on. */
function folderHeader(page: Page, name: string) {
  return page.locator('[data-sidebar-row^="folder:"]', { hasText: name });
}

/** The folder's Collapsible root: the header row and its content panel are
 *  siblings within it, so this — not the header row itself — is what a
 *  dropped-in item ends up a DOM descendant of. */
function folderRoot(page: Page, name: string) {
  return page
    .locator('[data-scope="collapsible"][data-part="root"]')
    .filter({ has: folderHeader(page, name) })
    .first();
}

const PROJECT_PATH = "/Users/ciaran/Projects/codex-custom";
const OTHER_PROJECT_PATH = "/Users/ciaran/Projects/arctic-explorer";

test.beforeEach(async ({ page }) => loadPreview(page));

test("sidebar rows stretch consistently and contain long names", async ({ page }) => {
  const first = projectRow(page, PROJECT_PATH);
  const second = projectRow(page, OTHER_PROJECT_PATH);
  await createFolder(page, "Work");

  async function expectAligned() {
    const boxes = await Promise.all([first, second, folderHeader(page, "Work")].map((row) => row.boundingBox()));
    expect(boxes.every(Boolean)).toBe(true);
    for (const box of boxes.slice(1)) {
      expect(Math.abs(box!.x - boxes[0]!.x)).toBeLessThanOrEqual(1);
      expect(Math.abs(box!.width - boxes[0]!.width)).toBeLessThanOrEqual(1);
    }
  }
  await expectAligned();

  await first.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Rename project", exact: true }).click();
  await page.getByPlaceholder("Project name").fill("codex-custom-permanent-worktree-with-a-very-long-name");
  await page.getByRole("button", { name: "Rename", exact: true }).click();
  await expect(first).toContainText("codex-custom-permanent-worktree-with-a-very-long-name");
  await expectAligned();

  // Check actual bounds, including children: clipping overflow would hide the
  // symptom without fixing the layout.
  async function expectContained() {
    const overflow = await page.locator("aside").first().evaluate((sidebar) => {
      const bounds = sidebar.getBoundingClientRect();
      return Array.from(sidebar.querySelectorAll('[data-sidebar-row], [data-part="trigger"], button[title$="open git"]'))
        .filter((element) => element.getClientRects().length > 0)
        .filter((element) => {
          const rect = element.getBoundingClientRect();
          return rect.left < bounds.left - 1 || rect.right > bounds.right + 1;
        })
        .map((element) => element.textContent);
    });
    expect(overflow).toEqual([]);
  }
  await expectContained();
  await first.hover();
  await expectContained();
  const trigger = first.locator('[data-scope="collapsible"][data-part="trigger"]');
  await trigger.click();
  await expectAligned();
  await trigger.click();
  await expectAligned();

  await folderHeader(page, "Work").click({ button: "right" });
  await page.getByRole("menuitem", { name: "New subfolder", exact: true }).click();
  await page.getByPlaceholder("Folder name").fill("Nested");
  await page.getByRole("button", { name: "Create", exact: true }).click();
  const outer = await folderHeader(page, "Work").boundingBox();
  const inner = await folderHeader(page, "Nested").boundingBox();
  expect(inner!.x).toBeGreaterThan(outer!.x);
  expect(Math.abs(inner!.x + inner!.width - outer!.x - outer!.width)).toBeLessThanOrEqual(1);
  await expectContained();

  await first.click({ button: "right" });
  await page.getByRole("menuitem", { name: "New thread folder", exact: true }).click();
  await page.getByPlaceholder("Folder name").fill("Thread notes");
  await page.getByRole("button", { name: "Create", exact: true }).click();
  const threadFolder = await folderHeader(page, "Thread notes").boundingBox();
  expect(Math.abs(threadFolder!.x - inner!.x)).toBeLessThanOrEqual(1);
  expect(Math.abs(threadFolder!.width - inner!.width)).toBeLessThanOrEqual(1);
  await expectContained();

  // Also exercise the minimum size in WebKit, whose configured project uses
  // the desktop viewport.
  await page.setViewportSize({ width: 820, height: 560 });
  await expectAligned();
  await expectContained();
});

test("dragging a project into a folder nests it immediately", async ({ page }) => {
  await createFolder(page, "Work");
  await expect(folderHeader(page, "Work")).toBeVisible();
  await expect(page.getByText("Drop projects here")).toBeVisible();

  await dragRow(page, projectRow(page, PROJECT_PATH), folderHeader(page, "Work"));

  // No artificial wait: the drop is optimistic, so this must be true the
  // instant the drag ends.
  const nested = folderRoot(page, "Work").locator(`[data-sidebar-row="item:${PROJECT_PATH}"]`);
  await expect(nested).toBeVisible();
  await expect(page.getByText("Drop projects here")).toHaveCount(0);
});

test("dragging a project back out of a folder un-nests it", async ({ page }) => {
  await createFolder(page, "Work");
  await dragRow(page, projectRow(page, PROJECT_PATH), folderHeader(page, "Work"));
  const nested = folderRoot(page, "Work").locator(`[data-sidebar-row="item:${PROJECT_PATH}"]`);
  await expect(nested).toBeVisible();

  // Drop it back onto the other root-level project, which lands it as a
  // root-scope sibling again.
  await dragRow(page, nested, projectRow(page, OTHER_PROJECT_PATH));

  await expect(nested).toHaveCount(0);
  await expect(projectRow(page, PROJECT_PATH)).toBeVisible();
});

test("reordering root projects via drag still works", async ({ page }) => {
  const first = projectRow(page, PROJECT_PATH);
  const second = projectRow(page, OTHER_PROJECT_PATH);
  await expect(first).toBeVisible();
  await expect(second).toBeVisible();

  await dragRow(page, first, second);

  const order = await page.locator("[data-sidebar-row^='item:']").allTextContents();
  expect(order.join(" ")).toContain("arctic-explorer");
});

test("hiding threads below folds them behind Show more until unhidden", async ({ page }) => {
  const first = page.locator('[data-sidebar-row="item:1"]');
  const second = page.locator('[data-sidebar-row="item:2"]');
  await expect(first).toBeVisible();
  await expect(second).toBeVisible();

  await first.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Hide threads below" }).click();
  await expect(second).toBeHidden();
  await expect(first).toBeVisible();

  await page.getByRole("button", { name: "Show 1 more" }).click();
  await expect(second).toBeVisible();
  await second.click({ button: "right" });
  await page.getByRole("menuitem", { name: "Unhide thread" }).click();
  // Nothing left to fold in this project (arctic-explorer keeps its own button).
  await expect(page.getByRole("button", { name: "Show 1 more" })).toBeHidden();
  await expect(second).toBeVisible();
});

test("reset project order undoes a drag", async ({ page, browserName }) => {
  const rows = () => page.locator("[data-sidebar-row^='item:/']");
  const before = await rows().allTextContents();
  await dragRow(page, projectRow(page, PROJECT_PATH), projectRow(page, OTHER_PROJECT_PATH));
  // Pointer-event drags don't land in WebKit (see the reorder test above).
  if (browserName !== "webkit") expect(await rows().allTextContents()).not.toEqual(before);

  await page.getByRole("button", { name: "Reset project order" }).click();
  await expect.poll(() => rows().allTextContents()).toEqual(before);
});

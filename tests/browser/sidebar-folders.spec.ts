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

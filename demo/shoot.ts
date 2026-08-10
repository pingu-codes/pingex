import { expect, type Page, type TestInfo } from "@playwright/test";

/** Where captured images land, one directory per Playwright project
 * (`light` / `dark`) so the same shot list produces both themes. */
export function shotDir(info: TestInfo): string {
  return `demo/screenshots/${info.project.name}`;
}

/** Save a full-window screenshot under a stable, sortable name. */
export async function shoot(page: Page, info: TestInfo, name: string): Promise<void> {
  // Let transitions, shimmer, and lazy git/status fetches settle first.
  await page.waitForTimeout(400);
  await page.screenshot({ path: `${shotDir(info)}/${name}.png` });
}

/** Boot the browser preview and wait until the sidebar has real data. */
export async function open(page: Page): Promise<void> {
  await page.goto("/");
  await expect(page.getByText("codex-custom", { exact: true }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "New thread", exact: true })).toBeVisible();
  // The branch chips resolve asynchronously; wait so no shot catches them empty.
  await expect(page.getByRole("button", { name: "main" }).first()).toBeVisible();
  await page.waitForTimeout(300);
}

/** Open the seeded conversation with reasoning, commands, and diffs. */
export async function openDemoThread(page: Page): Promise<void> {
  await page.getByRole("button", { name: /Custom frontend skeleton/ }).click();
  await expect(page.getByRole("button", { name: "Worked for 24s" })).toBeVisible();
}

/** Open the worktrees view for the first project. The branch chip that leads
 * there is swapped for row actions on hover, so dispatch the click directly
 * instead of moving the mouse onto it. */
export async function openWorktrees(page: Page): Promise<void> {
  await page
    .getByTitle(/open worktrees/)
    .first()
    .dispatchEvent("click");
  await expect(page.getByRole("button", { name: "New worktree" })).toBeVisible();
}

/** The thread overview card opens with the thread and floats over the right
 * panel; collapse it so panel content is unobstructed, as a user would. */
export async function closeOverview(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Thread overview" }).click();
}

/** Start an empty thread. The overview card only appears once the thread has a
 * turn, so callers that send a message collapse it afterwards. */
export async function newThread(page: Page): Promise<void> {
  await page.getByRole("button", { name: "New thread", exact: true }).click();
  await expect(composer(page)).toBeVisible();
}

/** Type into the composer without triggering the mention/slash pickers. */
export function composer(page: Page) {
  return page.getByRole("textbox", { name: /Message Codex/ });
}

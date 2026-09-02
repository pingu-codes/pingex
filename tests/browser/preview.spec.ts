import { expect, type Locator, type Page, test } from "@playwright/test";

async function loadPreview(page: Page) {
  await page.goto("/");
  await expect(page.getByText("codex-custom", { exact: true }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "New thread", exact: true })).toBeVisible();
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
}

async function selectPreviewThread(page: Page) {
  await page.getByRole("button", { name: /Custom frontend skeleton/ }).click();
  await expect(page.getByRole("button", { name: "Worked for 24s" })).toBeVisible();
}

function threadMenu(page: Page, title: string): Locator {
  return page
    .getByRole("button", { name: new RegExp(title) })
    .locator("..")
    .getByRole("button", { name: "Thread menu" });
}

test.beforeEach(async ({ page }) => loadPreview(page));

test("explores completed reasoning, command output, attachments, and diffs", async ({ page }) => {
  await selectPreviewThread(page);
  await expect(page.getByAltText("Attachment")).toBeVisible();
  await expect(page.getByText("The trailing call always wins", { exact: false })).toBeVisible();

  await page.getByRole("button", { name: "Worked for 24s" }).click();
  await expect(page.getByText("Planning the helper", { exact: false })).toBeVisible();

  const command = page.locator("button").filter({ hasText: "rg -n" });
  await command.click();
  await expect(page.getByText("export function clamp", { exact: false })).toBeVisible();

  const largeDiff = page.locator("button").filter({ hasText: "src/lib/generated.ts" });
  await largeDiff.click();
  await expect(page.getByRole("button", { name: /Show all 401 lines/ })).toBeVisible();
  await page.getByRole("button", { name: /Show all 401 lines/ }).click();
  await expect(page.getByText("+const value399 = 399;", { exact: false })).toBeVisible();
});

test("truncates a long project thread list until expanded", async ({ page }) => {
  await expect(page.getByRole("button", { name: /Ice core batch 14/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /Ice core batch 15/ })).toHaveCount(0);

  await page.getByRole("button", { name: /Show \d+ more/ }).click();
  await expect(page.getByRole("button", { name: /Ice core batch 18/ })).toBeVisible();

  await page.getByRole("button", { name: "Show less" }).click();
  await expect(page.getByRole("button", { name: /Ice core batch 15/ })).toHaveCount(0);
});

test("creates a thread and renders optimistic and streamed content", async ({ page }) => {
  await page.getByRole("button", { name: "New thread", exact: true }).click();
  const composer = page.getByRole("textbox", { name: /Message Codex/ });
  await composer.fill("Stream a preview response");
  await composer.press("Enter");

  await expect(page.getByText("Stream a preview response", { exact: true })).toBeVisible();
  await expect(page.getByText("Working…")).toBeVisible();
  await expect(page.getByText("Considering the request", { exact: false })).toBeVisible();
  await expect(page.getByText("Here is a preview response streamed", { exact: false })).toBeVisible();
  await expect(page.getByRole("button", { name: /Worked for/ })).toBeVisible();
});

test("answers an approval and lets the turn continue", async ({ page }) => {
  await page.getByRole("button", { name: "New thread", exact: true }).click();
  const composer = page.getByRole("textbox", { name: /Message Codex/ });
  await composer.fill("Please approve this command");
  await composer.press("Enter");

  await expect(page.getByText("Codex wants to run a command")).toBeVisible();
  await page.getByRole("button", { name: "Allow", exact: true }).click();
  await expect(page.getByText("Codex wants to run a command")).not.toBeVisible();
  await expect(page.getByText("Here is a preview response streamed", { exact: false })).toBeVisible();
});

test("interrupts an active turn and suppresses later stream chunks", async ({ page }) => {
  await page.clock.install();
  await page.getByRole("button", { name: "New thread", exact: true }).click();
  const composer = page.getByRole("textbox", { name: /Message Codex/ });
  await composer.fill("Start a turn that will be interrupted");
  await composer.press("Enter");

  await expect(page.getByRole("button", { name: "Interrupt" })).toBeVisible();
  await page.getByRole("button", { name: "Interrupt" }).click();
  await expect(page.getByRole("button", { name: "Send message" })).toBeVisible();
  await page.clock.fastForward(10_000);
  await expect(page.getByText("Here is a preview response streamed", { exact: false })).not.toBeVisible();
});

test("renders selected @ files as inline composer chips", async ({ page }) => {
  await page.getByRole("button", { name: "New thread", exact: true }).click();
  const composer = page.getByRole("textbox", { name: /Message Codex/ });

  await composer.fill("Add comments to @util");
  await page.getByRole("option", { name: /utils\.ts/ }).click();
  // The chip shows the bare path; the `@` belongs to the query, not the chip.
  await expect(composer.locator("[data-mention-path]")).toContainText("utils.ts");

  await composer.type(" please");
  await composer.press("Enter");
  await expect(page.getByText("Add comments to", { exact: false })).toBeVisible();
  // How the mention is echoed in the transcript is covered by
  // composer-rich-input.spec.ts, which owns that behaviour.
});

test("renames, pins, archives, and deletes threads", async ({ page }) => {
  await threadMenu(page, "Custom frontend skeleton").click();
  await page.getByRole("menuitem", { name: "Rename thread" }).click();
  const renameDialog = page.getByRole("dialog", { name: "Rename thread" });
  await renameDialog.getByRole("textbox").fill("Renamed preview thread");
  await renameDialog.getByRole("button", { name: "Rename", exact: true }).click();
  await expect(page.getByRole("button", { name: /Renamed preview thread/ })).toBeVisible();

  await threadMenu(page, "Renamed preview thread").click();
  await page.getByRole("menuitem", { name: "Favorite thread" }).click();
  await threadMenu(page, "Renamed preview thread").click();
  await expect(page.getByRole("menuitem", { name: "Unfavorite thread" })).toBeVisible();
  await page.keyboard.press("Escape");

  await threadMenu(page, "Renamed preview thread").click();
  await page.getByRole("menuitem", { name: "Archive thread" }).click();
  await expect(page.getByRole("button", { name: /Renamed preview thread/ })).not.toBeVisible();

  await threadMenu(page, "Tauri app-server bridge").click();
  await page.getByRole("menuitem", { name: "Delete thread" }).click();
  const deleteDialog = page.getByRole("dialog", { name: "Delete thread" });
  await deleteDialog.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(page.getByRole("button", { name: /Tauri app-server bridge/ })).not.toBeVisible();
});

test("shows spawned agents in the transcript and in the subagent menu", async ({ page }) => {
  await selectPreviewThread(page);
  await page.getByRole("button", { name: "Worked for 24s" }).click();

  // One agent finished, one is still going — both render as agent cards in the
  // transcript, and only the running one offers a Stop.
  const spawned = page.getByText("Spawned agent");
  await expect(spawned).toHaveCount(2);
  await expect(spawned.first()).toContainText("debounce audit");
  await expect(spawned.last()).toContainText("test sweep");
  await expect(page.getByRole("button", { name: "Stop", exact: true })).toHaveCount(1);

  // The app's runs are merged into the same subagent tree Codex's own use.
  await expect(page.getByRole("button", { name: "Open subagent debounce audit" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Open subagent test sweep" })).toBeVisible();
  // Only the running one can be stopped, and only because we own its process.
  await expect(page.getByRole("button", { name: /^Stop subagent/ })).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Stop subagent test sweep" })).toBeVisible();
});

import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "New thread", exact: true })).toBeVisible();
  await page.getByTestId("sidebar-memories").click();
  await expect(page.getByTestId("memories-view")).toBeVisible();
});

test("lists both Homes and opens the first index", async ({ page }) => {
  const view = page.getByTestId("memories-view");
  await expect(view.getByText("Codex", { exact: true })).toBeVisible();
  await expect(view.getByText("Claude Code", { exact: true })).toBeVisible();
  await expect(view.getByRole("heading", { name: "Memory", exact: true })).toBeVisible();
  await expect(page.getByTestId("sidebar-memories")).toHaveAttribute("aria-current", "page");
});

test("edits, searches and deletes a Claude memory", async ({ page }) => {
  const view = page.getByTestId("memories-view");
  await view.getByRole("button", { name: /verification-gotchas/ }).click();
  await expect(view).toContainText("reformats untouched files");
  await expect(view).not.toContainText("metadata:");

  await view.getByRole("button", { name: "Edit memory" }).click();
  await view.getByLabel("Memory contents").fill("Format only the files you changed.");
  await view.getByRole("button", { name: "Save", exact: true }).click();
  await expect(view.getByRole("paragraph").filter({ hasText: "Format only the files you changed." })).toBeVisible();

  await view.getByLabel("Search memories").fill("sidebar folders");
  await expect(view.getByRole("button", { name: /verification-gotchas/ })).toHaveCount(0);
  await expect(view.getByRole("button", { name: /sidebar-folders/ })).toBeVisible();
  await view.getByLabel("Search memories").fill("");

  await view.getByRole("button", { name: /verification-gotchas/ }).click();
  await view.getByRole("button", { name: "Delete memory" }).click();
  await view.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(view.getByRole("button", { name: /verification-gotchas/ })).toHaveCount(0);
});

test("flips a memory switch", async ({ page }) => {
  const toggle = page.getByRole("switch", { name: "Codex: Use memories" });
  await expect(toggle).toBeChecked();
  // The checkbox is visually hidden under the switch's track, which takes the click.
  await toggle.uncheck({ force: true });
  await expect(toggle).not.toBeChecked();
});

test("/memories opens the page from a composer", async ({ page }) => {
  await page.getByRole("button", { name: "New thread", exact: true }).click();
  const composer = page.getByRole("textbox").last();
  await composer.click();
  await composer.pressSequentially("/memories");
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("memories-view")).toBeVisible();
});

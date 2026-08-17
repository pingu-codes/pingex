import { expect, type Page, test } from "@playwright/test";

async function loadPreview(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "New thread", exact: true })).toBeVisible();
}

async function selectPreviewThread(page: Page) {
  await page.getByRole("button", { name: /Custom frontend skeleton/ }).click();
  await expect(page.getByRole("button", { name: "Worked for 24s" })).toBeVisible();
}

test.beforeEach(async ({ page }) => loadPreview(page));

test("shows a retirement badge for a model scheduled to retire", async ({ page }) => {
  await selectPreviewThread(page);
  await page.getByRole("button", { name: "Select model and effort" }).click();
  await expect(page.getByText(/Retiring/)).toBeVisible();
});

test("shows the estimated usage section in /status", async ({ page }) => {
  await selectPreviewThread(page);
  const composer = page.getByRole("textbox", { name: /Message Codex/ });
  await composer.click();
  await composer.pressSequentially("/status");
  await page.keyboard.press("Enter");
  await expect(page.getByText("Estimated usage")).toBeVisible();
  await expect(page.getByText("Credits")).toBeVisible();
});

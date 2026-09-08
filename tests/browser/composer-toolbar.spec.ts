import { expect, test } from "@playwright/test";

test("keeps composer controls on one line through overview, panel and width changes", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: /Custom frontend skeleton/ }).click();
  const toolbar = page.getByTestId("composer-toolbar");
  async function fits() {
    await expect
      .poll(() =>
        toolbar.evaluate((root) => {
          const bounds = root.getBoundingClientRect();
          const controls = [...root.children].filter((el) => !el.classList.contains("overflow"));
          return controls.every((el) => {
            const rect = el.getBoundingClientRect();
            return rect.left >= bounds.left - 1 && rect.right <= bounds.right + 1 && rect.height <= 32;
          });
        }),
      )
      .toBe(true);
  }
  await fits();
  await page.getByRole("button", { name: "Thread overview", exact: true }).click();
  await fits();
  await page.getByRole("button", { name: "Thread overview", exact: true }).click();
  await page.getByTitle("View diff for src/lib/utils.ts").click();
  await fits();
  for (const width of [820, 1180, 960, 820, 1400]) {
    await page.setViewportSize({ width, height: 760 });
    await fits();
  }
});

test("overflow settings remain operable and Escape restores focus", async ({ page }) => {
  await page.setViewportSize({ width: 820, height: 560 });
  await page.goto("/");
  await page.getByRole("button", { name: "New thread", exact: true }).click();
  const more = page.getByRole("button", { name: "More composer options" });
  await more.click();
  const dialog = page.getByRole("dialog", { name: "More composer options", exact: true });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Set permissions level" }).click();
  const permissions = page.getByRole("dialog", { name: "Permission options" });
  await expect(permissions).toBeVisible();
  await permissions.getByRole("button").first().click();
  await dialog.getByRole("button", { name: "Set permissions level" }).click();
  const rect = await permissions.boundingBox();
  expect(rect!.x).toBeGreaterThanOrEqual(0);
  expect(rect!.y).toBeGreaterThanOrEqual(0);
  await page.keyboard.press("Escape");
  await expect(dialog).not.toBeVisible();
  await expect(more).toBeFocused();
});

test("long labels and keyboard commands work with overflow", async ({ page }) => {
  await page.setViewportSize({ width: 820, height: 560 });
  await page.goto("/");
  await page.getByRole("button", { name: "New thread", exact: true }).click();
  const model = page.getByRole("button", { name: "Select model and effort" });
  await model.locator("span").evaluate((el) => {
    el.textContent = "A very long model display name with a long reasoning effort";
  });
  const bounds = await model.boundingBox();
  expect(bounds!.height).toBeLessThanOrEqual(32);
  await page.getByRole("textbox", { name: /Message Codex/ }).fill("/permissions");
  await page.getByRole("textbox", { name: /Message Codex/ }).press("Enter");
  await expect(page.getByRole("dialog", { name: "More composer options", exact: true })).toBeVisible();
  await expect(page.getByRole("dialog", { name: "Permission options" })).toBeVisible();
  await page.screenshot({ path: test.info().outputPath("toolbar.png") });
});

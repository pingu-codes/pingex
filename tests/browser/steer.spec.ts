import { expect, test } from "@playwright/test";
import { chooseTurnSettings } from "./turnSettings";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
});

test("Enter steers the running turn and Tab queues for after it", async ({ page }) => {
  await page.getByRole("button", { name: "New thread", exact: true }).click();
  await chooseTurnSettings(page);
  const composer = page.getByRole("textbox", { name: /Message Codex/ });
  await composer.fill("Stream a preview response");
  await composer.press("Enter");
  await expect(page.getByText("Working…")).toBeVisible();
  await expect(page.getByRole("button", { name: "Steer" })).toBeVisible();

  await composer.fill("Also check the tests");
  await composer.press("Enter");
  // The steer lands in the running turn's transcript, not in the queue.
  await expect(page.getByText("Also check the tests", { exact: true })).toBeVisible();
  await expect(page.getByText("Queued", { exact: true })).toHaveCount(0);

  await composer.fill("Then write the changelog");
  await composer.press("Tab");
  await expect(page.getByText("Then write the changelog", { exact: true })).toBeVisible();
  await expect(composer).toBeFocused();
});

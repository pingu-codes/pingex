import type { Page } from "@playwright/test";

/** A fresh profile has no model or permission mode, and the composer refuses
 *  to send until both are chosen ("Choose a model"). Pick the preview's
 *  default model and the Auto preset. */
export async function chooseTurnSettings(page: Page): Promise<void> {
  await page.getByRole("button", { name: "Select model and effort" }).click();
  await page
    .getByRole("dialog", { name: "Model options" })
    .getByRole("button", { name: /^GPT-5\.2 Codex/ })
    .click();
  await page.getByRole("button", { name: "Set permissions level" }).click();
  await page.getByRole("dialog", { name: "Permission options" }).getByRole("button", { name: /^Auto/ }).click();
}

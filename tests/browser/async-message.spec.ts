import { expect, test } from "@playwright/test";

for (const theme of ["light", "dark"]) {
  test(`async messages have a stable hover and focus outline in ${theme} mode`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: theme === "dark" ? "dark" : "light" });
    await page.goto("/");
    await expect(page.getByRole("button", { name: /Custom frontend skeleton/ })).toBeVisible();
    await page.evaluate(async () => {
      // Use the same saved-history fixture consumed when the thread opens.
      const modulePath = "/src/lib/services/preview/fixtures.ts";
      const { previewThread } = await import(/* @vite-ignore */ modulePath);
      previewThread.turns[0].items.push({
        id: "async-appearance",
        type: "agentMessage",
        delivery: "async",
        phase: "final_answer",
        text: "Where is the detailed sample example?",
      });
    });
    await page.getByRole("button", { name: /Custom frontend skeleton/ }).click();
    const badge = page.getByText("Async", { exact: true });
    await expect(badge).toBeVisible();
    const message = badge.locator("..");
    await message.scrollIntoViewIfNeeded();
    await page.mouse.move(0, 0);
    const before = await message.boundingBox();
    await expect(message).toHaveCSS("border-top-color", "rgba(0, 0, 0, 0)");
    await badge.hover();
    await expect(message).not.toHaveCSS("border-top-color", "rgba(0, 0, 0, 0)");
    expect(await message.boundingBox()).toEqual(before);
    await message.evaluate((element) => Promise.all(element.getAnimations().map((animation) => animation.finished)));
    await page.screenshot({ path: `/tmp/pingex-async-${theme}.png` });
    await page.mouse.move(0, 0);
    await message.getByRole("button", { name: "Copy message", exact: true }).focus();
    await expect(message).not.toHaveCSS("border-top-color", "rgba(0, 0, 0, 0)");
    expect(await message.boundingBox()).toEqual(before);
  });
}

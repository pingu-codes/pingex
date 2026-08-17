import { expect, type Locator, type Page, test } from "@playwright/test";

async function loadPreview(page: Page) {
  await page.goto("/");
  await expect(page.getByText("codex-custom", { exact: true }).first()).toBeVisible();
  await expect(page.getByRole("button", { name: "New thread", exact: true })).toBeVisible();
}

async function newComposer(page: Page): Promise<Locator> {
  await page.getByRole("button", { name: "New thread", exact: true }).click();
  return page.getByRole("textbox", { name: /Message Codex/ });
}

/** Insert a mention chip for `utils.ts` by typing `@util` and picking the result. */
async function insertUtilsMention(page: Page, composer: Locator) {
  await composer.pressSequentially("@util");
  await page.getByRole("option", { name: /utils\.ts/ }).click();
}

/** Insert a mention chip for `api.ts` by typing `@api` and picking the result. */
async function insertApiMention(page: Page, composer: Locator) {
  await composer.pressSequentially("@api");
  await page.getByRole("option", { name: /api\.ts/ }).click();
}

/** Collapses the selection to just before/after the (sole) mention chip, bypassing key-count caret math. */
async function placeCaretBesideChip(composer: Locator, side: "before" | "after") {
  await composer.evaluate((root, side) => {
    const chip = root.querySelector<HTMLElement>("[data-mention-path]");
    if (!chip) throw new Error("no mention chip in composer");
    const range = document.createRange();
    if (side === "before") range.setStartBefore(chip);
    else range.setStartAfter(chip);
    range.collapse(true);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  }, side);
}

/** Clicks the transcript area above the composer — somewhere inert and outside it. */
async function clickOutsideComposer(page: Page, composer: Locator) {
  const box = await composer.boundingBox();
  if (!box) throw new Error("composer is not visible");
  await page.mouse.click(box.x + box.width / 2, box.y - 100);
}

/** Places the caret at the end of the composer's first text node. */
async function placeCaretAtEndOfFirstText(composer: Locator) {
  await composer.evaluate((root) => {
    const textNode = [...root.childNodes].find((node): node is Text => node.nodeType === Node.TEXT_NODE);
    if (!textNode) throw new Error("no leading text node in composer");
    const range = document.createRange();
    range.setStart(textNode, textNode.textContent?.length ?? 0);
    range.collapse(true);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  });
}

test.beforeEach(async ({ page }) => loadPreview(page));

test("renders a mention chip inline with the label and file icon", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("Add comments to ");
  await insertUtilsMention(page, composer);
  const chip = composer.locator("[data-mention-path]");
  await expect(chip).toHaveCount(1);
  await expect(chip).toContainText("utils.ts");
  await expect(chip).toHaveAttribute("data-mention-path", /utils\.ts$/);
});

test("preserves multiple chips and surrounding text in order", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("before ");
  await insertUtilsMention(page, composer);
  await composer.pressSequentially(" middle ");
  await insertApiMention(page, composer);
  await composer.pressSequentially(" after");

  const chips = composer.locator("[data-mention-path]");
  await expect(chips).toHaveCount(2);
  await expect(chips.nth(0)).toContainText("utils.ts");
  await expect(chips.nth(1)).toContainText("api.ts");
  await expect(composer).toContainText("before utils.ts middle api.ts after");
});

test("Backspace removes a chip atomically, adjacent text untouched", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("before ");
  await insertUtilsMention(page, composer);
  await composer.pressSequentially(" after");
  await expect(composer.locator("[data-mention-path]")).toHaveCount(1);

  await placeCaretBesideChip(composer, "after");
  await composer.press("Backspace");

  await expect(composer.locator("[data-mention-path]")).toHaveCount(0);
  await expect(composer).toContainText("before");
  await expect(composer).toContainText("after");
});

test("Delete removes a chip atomically", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("before ");
  await insertUtilsMention(page, composer);
  await composer.pressSequentially(" after");
  await expect(composer.locator("[data-mention-path]")).toHaveCount(1);

  await placeCaretBesideChip(composer, "before");
  await composer.press("Delete");

  await expect(composer.locator("[data-mention-path]")).toHaveCount(0);
  await expect(composer).toContainText("before");
  await expect(composer).toContainText("after");
});

test("ArrowRight from just before a chip skips fully over it in one step", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("before ");
  await insertUtilsMention(page, composer);
  await composer.pressSequentially("after");

  await placeCaretBesideChip(composer, "before");
  await composer.press("ArrowRight");
  await composer.pressSequentially("X");

  // A single ArrowRight jumped past the whole chip: "X" landed right after it,
  // not inside/on it.
  await expect(composer).toContainText("before utils.tsXafter");
});

test("Cmd+ArrowRight (line-edge) hops over a chip in one step", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("before ");
  await insertUtilsMention(page, composer);
  await composer.pressSequentially("after");

  await placeCaretBesideChip(composer, "before");
  await composer.press("Meta+ArrowRight");
  await composer.pressSequentially("X");

  await expect(composer).toContainText("before utils.tsafterX");
});

test("Cmd+ArrowRight crosses a skill chip, not just a mention chip", async ({ page }) => {
  // Skill chips were invisible to the chip-crossing helpers, so Cmd+ArrowRight
  // parked the caret against one and went no further.
  const composer = await newComposer(page);
  await composer.pressSequentially("before $code");
  await page.getByRole("option", { name: /code-reviewer/ }).click();
  await composer.pressSequentially("after");

  await composer.evaluate((root) => {
    const chip = root.querySelector<HTMLElement>("[data-skill-name]");
    if (!chip) throw new Error("no skill chip in composer");
    const range = document.createRange();
    range.setStartBefore(chip);
    range.collapse(true);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  });
  await composer.press("Meta+ArrowRight");
  await composer.pressSequentially("X");

  await expect(composer).toContainText("afterX");
  await expect(composer.locator("[data-skill-name]")).toHaveCount(1);
});

test("Option+ArrowRight steps over a chip as a single word", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("before ");
  await insertUtilsMention(page, composer);
  await composer.pressSequentially(" tail");

  await placeCaretBesideChip(composer, "before");
  await composer.press("Alt+ArrowRight");
  await composer.pressSequentially("X");

  // One Option+ArrowRight takes the whole chip, landing right after it rather
  // than stalling against it or walking into the middle of its label.
  await expect(composer).toContainText("before utils.tsX tail");
});

test("Option+ArrowLeft steps back over a chip as a single word", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("before ");
  await insertUtilsMention(page, composer);

  await placeCaretBesideChip(composer, "after");
  await composer.press("Alt+ArrowLeft");
  await composer.pressSequentially("X");

  // Lands just before the chip, keeping the space — the same span
  // Option+Backspace would have taken.
  await expect(composer).toContainText("before X");
  await expect(composer.locator("[data-mention-path]")).toHaveCount(1);
});

test("Cmd+Backspace (delete-to-line-edge) deletes through a chip in one keystroke", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("before ");
  await insertUtilsMention(page, composer);
  await composer.pressSequentially("after");

  await placeCaretBesideChip(composer, "after");
  await composer.press("Meta+Backspace");

  await expect(composer.locator("[data-mention-path]")).toHaveCount(0);
  await expect(composer).toContainText("after");
  await expect(composer).not.toContainText("before");
});

test("Option+Delete over a lone space next to a chip doesn't strand the caret at the start", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially(" ");
  await insertUtilsMention(page, composer);
  await composer.pressSequentially(" after");

  // Caret at the very start, right before the lone leading space.
  await composer.evaluate((root) => {
    const textNode = [...root.childNodes].find((node): node is Text => node.nodeType === Node.TEXT_NODE);
    if (!textNode) throw new Error("no leading text node in composer");
    const range = document.createRange();
    range.setStart(textNode, 0);
    range.collapse(true);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  });
  await composer.press("Alt+Delete");
  await composer.pressSequentially("Z");

  // WebKit's native word-delete has been seen to turn that lone space into a
  // stray line break and pin the caret to the very start of the composer —
  // "Z" would then land before a blank line rather than before " after".
  await expect(composer.locator("br")).toHaveCount(0);
  await expect(composer.locator("[data-mention-path]")).toHaveCount(0);
  await expect(composer).toContainText("Z after");
});

test("Shift+Enter beside a chip produces exactly one line break", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("before ");
  await insertUtilsMention(page, composer);
  await composer.pressSequentially("after");

  await placeCaretBesideChip(composer, "after");
  await composer.press("Shift+Enter");
  await composer.pressSequentially("next line");

  const brCount = await composer.evaluate((root) => root.querySelectorAll("br").length);
  // Exactly one break was inserted — the intercepted path guards against WebKit
  // writing two <br>s beside a contenteditable=false chip.
  expect(brCount).toBe(1);
  await expect(composer.locator("[data-mention-path]")).toHaveCount(1);
  await expect(composer).toContainText("before");
  await expect(composer).toContainText("next line");
  await expect(composer).toContainText("after");
});

test("ArrowRight crosses a Shift+Enter break typed just before a chip", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("hello ");
  await insertUtilsMention(page, composer);

  // Break the line between the text and the chip, so the chip becomes the
  // first (and only) thing on the line below.
  await placeCaretBesideChip(composer, "before");
  await composer.press("Shift+Enter");

  // Move the caret back into "hello " (as if the user clicked back into it),
  // then try to walk forward past the break onto the chip's line.
  await placeCaretAtEndOfFirstText(composer);
  await composer.press("ArrowRight");
  await composer.press("ArrowRight");
  await composer.pressSequentially("X");

  // WebKit refuses to move the caret across a break when a chip with no text
  // of its own sits immediately on the other side — without the composer's
  // own handling, both ArrowRights are silently swallowed and "X" lands back
  // inside "hello " instead of after the chip.
  await expect(composer).toContainText("hello");
  await expect(composer).toContainText("utils.tsX");
});

test("ArrowDown crosses a Shift+Enter break typed just before a chip", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("hello ");
  await insertUtilsMention(page, composer);

  // Break the line between the text and the chip, so the chip becomes the
  // first (and only) thing on the line below.
  await placeCaretBesideChip(composer, "before");
  await composer.press("Shift+Enter");

  await placeCaretAtEndOfFirstText(composer);
  await composer.press("ArrowDown");
  await composer.pressSequentially("X");

  // Same WebKit refusal as ArrowRight, but for vertical motion: with only a
  // chip on the line below, there's no column to land the caret on and it
  // does nothing at all. "hello "'s column (6) overshoots the chip's line
  // (length 1), so the caret should land past the chip, not stuck above it.
  await expect(composer).toContainText("hello");
  await expect(composer).toContainText("utils.tsX");
});

test("ArrowDown from the middle of the line above lands past the chip, not stuck", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("this is a fairly long first line of text");
  await placeCaretAtEndOfFirstText(composer);
  await composer.press("Shift+Enter");
  await insertUtilsMention(page, composer);

  // A middling column, nowhere near the end of line 1 (so the old fix, which
  // only handled the caret sitting right before the break, would miss this).
  await composer.evaluate((root) => {
    const textNode = root.firstChild as Text;
    const range = document.createRange();
    range.setStart(textNode, 10);
    range.collapse(true);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  });
  await composer.press("ArrowDown");
  await composer.pressSequentially("X");

  await expect(composer).toContainText("this is a fairly long first line of text");
  await expect(composer).toContainText("utils.tsX");
});

test("attachment chip staging→ready lifecycle via the browser file input", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.click();
  const fileInput = page.locator('input[type="file"]');
  await fileInput.setInputFiles({ name: "notes.txt", mimeType: "text/plain", buffer: Buffer.from("hello world") });

  const chip = composer.locator("[data-attachment-id]");
  await expect(chip).toHaveCount(1);
  await expect(chip).toContainText("notes.txt");
  // The size label only renders once staging resolves to "ready".
  await expect(chip.getByText(/^\d+(\.\d+)? (B|KB|MB)$/)).toBeVisible();
});

test("a chip carries no remove button — Backspace is the only way out", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });

  const composer = await newComposer(page);
  await composer.pressSequentially("keep ");
  await insertUtilsMention(page, composer);

  const chip = composer.locator("[data-mention-path]");
  await expect(chip).toHaveCount(1);
  await chip.hover();
  await expect(chip.getByRole("button")).toHaveCount(0);

  await placeCaretBesideChip(composer, "after");
  await composer.press("Backspace");
  await expect(chip).toHaveCount(0);
  await expect(composer).toContainText("keep");
  expect(errors).toEqual([]);
});

test("a chip does not make its line taller than a line of plain text", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("plain line");
  await placeCaretAtEndOfFirstText(composer);
  await composer.press("Shift+Enter");
  await insertUtilsMention(page, composer);
  await composer.pressSequentially(" chip line");

  const chip = composer.locator("[data-mention-path]");
  const chipHeight = await chip.evaluate((node) => node.getBoundingClientRect().height);
  const lineHeight = await composer.evaluate((root) => Number.parseFloat(getComputedStyle(root).lineHeight));

  // The chip has to fit inside the editor's line box, or every line holding
  // one grows and the composer jitters as chips are inserted and removed.
  expect(chipHeight).toBeLessThanOrEqual(lineHeight);
});

test("sends a message with a mention and a skill chip, echoed correctly in the transcript", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("Review ");
  await insertUtilsMention(page, composer);
  await composer.pressSequentially(" using $code");
  await page.getByRole("option", { name: /code-reviewer/ }).click();
  await composer.press("Enter");

  await expect(page.getByText("Review @utils.ts using", { exact: false })).toBeVisible();
  await expect(page.getByText("@utils.ts", { exact: true }).last()).toBeVisible();
  await expect(page.getByText("code-reviewer", { exact: true }).last()).toBeVisible();
});

test("draft persistence restores chips after a reload", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("remember ");
  await insertUtilsMention(page, composer);
  await composer.pressSequentially(" please");
  await expect(composer.locator("[data-mention-path]")).toHaveCount(1);

  // Drafts save on a 400ms debounce.
  await page.waitForTimeout(700);
  await page.reload();
  await loadPreview(page);
  const restored = await newComposer(page);
  await expect(restored.locator("[data-mention-path]")).toContainText("utils.ts");
  await expect(restored).toContainText("remember");
  await expect(restored).toContainText("please");
});

test("clicking outside the composer dismisses the mention picker but keeps the text", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("look at @util");
  await expect(page.getByRole("listbox", { name: "Attach a project file or folder" })).toBeVisible();

  await clickOutsideComposer(page, composer);

  await expect(page.getByRole("listbox", { name: "Attach a project file or folder" })).toHaveCount(0);
  await expect(composer).toContainText("look at @util");
});

test("clicking outside the composer dismisses the slash picker", async ({ page }) => {
  const composer = await newComposer(page);
  await composer.pressSequentially("/fo");
  await expect(page.getByRole("listbox", { name: "Slash commands" })).toBeVisible();

  await clickOutsideComposer(page, composer);

  await expect(page.getByRole("listbox", { name: "Slash commands" })).toHaveCount(0);
});

test("pressing a picker row still selects it, rather than the press dismissing the picker", async ({ page }) => {
  // WebKit does not focus a button on mousedown, so a focus-based dismissal
  // would unmount the row before its click landed. This is the guard for that.
  const composer = await newComposer(page);
  await insertUtilsMention(page, composer);

  await expect(composer.locator("[data-mention-path]")).toContainText("utils.ts");
  await expect(page.getByRole("listbox", { name: "Attach a project file or folder" })).toHaveCount(0);
});

test("plan mode is restored after a reload", async ({ page }) => {
  await newComposer(page);
  const planMode = page.getByRole("button", { name: "Toggle plan mode" });
  await planMode.click();
  await expect(planMode).toHaveAttribute("aria-pressed", "true");

  await page.reload();
  await loadPreview(page);
  await newComposer(page);

  await expect(page.getByRole("button", { name: "Toggle plan mode" })).toHaveAttribute("aria-pressed", "true");
});

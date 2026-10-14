/**
 * DOM implementation of the composer's rich input: parts <-> contenteditable,
 * chips, caret motion and the editing verbs WebKit cannot be trusted with.
 *
 * Internal to `RichEditor` (`richEditor.svelte.ts`): only that module and the
 * tests import from here. The parts model itself lives in `composerParts.ts`.
 */

import {
  type AttachmentChipHandlers,
  type AttachmentPart,
  type ChipPart,
  type ComposerPart,
  type DetectedQueries,
  normaliseParts,
} from "$lib/composer/composerParts";
import { detectSlashQuery } from "$lib/composer/slashCommands";
import { fileIconFor, fileIconSvg, iconForPath } from "$lib/utils/fileIcons";
import { formatBytes } from "$lib/utils/format";

/** One caret unit of flattened content: a character or a whole chip. */
type Unit = string | ComposerPart;
type Units = Unit[];

const isBlock = (node: Node): boolean => /^(DIV|P)$/.test(node.nodeName);

/** Every chip carries one of these; see `readChip` for what each holds. */
const CHIP_SELECTOR = "[data-mention-path],[data-attachment-id],[data-skill-name]";

const asChip = (node: Node | null | undefined): HTMLElement | null =>
  node instanceof HTMLElement && node.matches(CHIP_SELECTOR) ? node : null;

/** The selection's first range, or null when there is none. */
export function liveRange(): Range | null {
  const selection = window.getSelection();
  return selection?.rangeCount ? selection.getRangeAt(0) : null;
}

/** The selection's range when it is a collapsed caret inside `root`. */
export function caretRangeIn(root: HTMLElement): Range | null {
  const range = liveRange();
  return range?.collapsed && root.contains(range.startContainer) ? range : null;
}

/** Collapses `range` to its start (or end) and makes it the selection. */
export function selectCollapsed(range: Range, toStart = true): void {
  range.collapse(toStart);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
}

/**
 * A `<br>` that ends its block renders no line of its own — it is the filler
 * every engine keeps at the end of an editable line. Only breaks with visible
 * content after them count as newlines.
 */
function isFillerBreak(node: Node): boolean {
  const next = solidSibling(node, "forward");
  return next === null || isBlock(next);
}

/**
 * Walks the contenteditable DOM and converts it back into composer parts.
 *
 * Line breaks are the subtle part: the composer writes `<br>`s, but browsers
 * answer Enter/Shift+Enter with `<div>` line wrappers, and a block boundary is
 * a break *before* the block, not after it. So a newline is emitted between two
 * siblings whenever either of them is a block, and never for a filler `<br>`.
 */
export function readParts(root: HTMLElement): ComposerPart[] {
  const walk = (node: Node): ComposerPart[] => {
    if (node.nodeType === Node.TEXT_NODE) return [{ type: "text", text: node.textContent ?? "" }];
    const chip = node instanceof HTMLElement ? readChip(node) : null;
    if (chip) return [chip];
    if (node.nodeName === "BR") return isFillerBreak(node) ? [] : [{ type: "text", text: "\n" }];
    return childParts(node);
  };
  const childParts = (node: Node): ComposerPart[] => {
    const parts: ComposerPart[] = [];
    let previous: Node | null = null;
    for (const child of node.childNodes) {
      if (isEmptyText(child)) continue;
      if (previous && (isBlock(child) || isBlock(previous))) parts.push({ type: "text", text: "\n" });
      parts.push(...walk(child));
      previous = child;
    }
    return parts;
  };
  return normaliseParts(childParts(root));
}

/**
 * Inspects the text before the caret for an active /-command, @-mention, or
 * $-skill query. Returns null when the caret is unavailable (no selection,
 * selection not collapsed, or caret outside the editor) so callers can keep
 * prior state.
 *
 * `@` and `$` may appear anywhere after whitespace; `/` only at the very start,
 * since a command is the whole message.
 */
export function detectQueries(root: HTMLElement, cwd: string): DetectedQueries | null {
  const range = caretRangeIn(root);
  if (!range) return null;
  const lastTextNode = (node: Node): Text | null => {
    if (node.nodeType === Node.TEXT_NODE) return node as Text;
    for (let index = node.childNodes.length - 1; index >= 0; index -= 1) {
      const found = lastTextNode(node.childNodes[index]);
      if (found) return found;
    }
    return null;
  };
  const textNode =
    range.startContainer.nodeType === Node.TEXT_NODE
      ? (range.startContainer as Text)
      : lastTextNode(range.startContainer.childNodes[range.startOffset - 1] ?? range.startContainer);
  const caretOffset =
    range.startContainer.nodeType === Node.TEXT_NODE ? range.startOffset : (textNode?.textContent?.length ?? 0);
  const before = textNode?.textContent?.slice(0, caretOffset) ?? "";
  // The rest of the token after the caret belongs to the query too: arrowing
  // back into `@.rs` and typing `lib` is asking for `lib.rs`.
  const after = textNode?.textContent?.slice(caretOffset) ?? "";
  const slashQuery = detectSlashQuery(before);
  const empty = { slashQuery, mentionQuery: null, mentionRange: null, skillQuery: null, skillRange: null };

  /** The range covering the trigger character, the query typed before the
   *  caret and the rest of its token after it. */
  const rangeFor = (head: string, tail: string): Range | null => {
    if (!textNode) return null;
    const range = document.createRange();
    range.setStart(textNode, caretOffset - head.length - 1);
    range.setEnd(textNode, caretOffset + tail.length);
    return range;
  };

  const mention = before.match(/(?:^|\s)@([\w./-]*)$/);
  if (mention && cwd && textNode) {
    const tail = after.match(/^[\w./-]*/)?.[0] ?? "";
    return { ...empty, mentionQuery: mention[1] + tail, mentionRange: rangeFor(mention[1], tail) };
  }
  // Skill names are namespaced (`browser-use:browser`), so `:` is part of the
  // query rather than a terminator.
  const skill = before.match(/(?:^|\s)\$([\w:.-]*)$/);
  if (skill && textNode) {
    const tail = after.match(/^[\w:.-]*/)?.[0] ?? "";
    return { ...empty, skillQuery: skill[1] + tail, skillRange: rangeFor(skill[1], tail) };
  }
  return empty;
}

/** The chip Backspace would take next (see `RichEditor`): a faint ring, so the
 *  whole-chip delete is not a surprise. */
const CHIP_CARET_ADJACENT = "data-[caret-adjacent]:ring-1 data-[caret-adjacent]:ring-primary-500/60";

/**
 * Shared shape for every chip: a quiet inline tag on a neutral surface, with
 * only its icon carrying colour.
 *
 * The fixed `h-5` (20px) and `leading-none` keep a chip inside the editor's
 * 24px `leading-6` line box, so a line holding one is exactly as tall as a line
 * of plain text. `align-middle` rather than `align-baseline` for the same
 * reason: an `inline-flex` baseline-aligns on its *inner* first-line baseline,
 * which hangs the box below the surrounding text and grows the line.
 */
const CHIP_SHAPE = `mx-0.5 inline-flex h-5 max-w-[16rem] select-none items-center gap-1 rounded-md px-1.5 align-middle text-[0.8125rem] leading-none transition-shadow ${CHIP_CARET_ADJACENT}`;
const CHIP_CLASS = `${CHIP_SHAPE} bg-surface-200-800`;

/** Sparkles, matching how a skill renders in the transcript. */
const SKILL_ICON =
  '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="size-3.5 shrink-0"><path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z"/></svg>';

/** Reads a chip's dataset back into the part it was built from. */
function readChip(node: HTMLElement): ChipPart | null {
  const { dataset } = node;
  if (dataset.attachmentId) {
    const attachment = readAttachmentDataset(node);
    if (attachment) return attachment;
  }
  if (dataset.mentionPath) return { type: "mention", name: dataset.mentionName ?? "file", path: dataset.mentionPath };
  if (dataset.skillName) {
    const name = dataset.skillName;
    return { type: "skill", name, path: dataset.skillPath ?? "", label: dataset.skillLabel ?? name };
  }
  return null;
}

/** Reads an attachment chip's serialized dataset back into a typed part. */
function readAttachmentDataset(node: HTMLElement): AttachmentPart | null {
  try {
    const raw = node.dataset.attachment;
    if (!raw) return null;
    const parsed = JSON.parse(raw) as AttachmentPart;
    return { ...parsed, type: "attachment" };
  } catch {
    return null;
  }
}

/** A chip's leading glyph. */
function chipIcon(className: string, svg: string): HTMLElement {
  const icon = document.createElement("span");
  icon.className = className;
  icon.innerHTML = svg;
  return icon;
}

/**
 * The span every chip shares: not editable, its part in the dataset, a lead
 * glyph and a truncating label. Chips have no remove control: Backspace/Delete
 * beside one takes the lot.
 */
function chipShell(
  data: Record<string, string>,
  className: string,
  title: string,
  lead: HTMLElement,
  text: string,
): HTMLElement {
  const chip = document.createElement("span");
  chip.contentEditable = "false";
  Object.assign(chip.dataset, data);
  chip.className = className;
  chip.title = title;
  const label = document.createElement("span");
  label.className = "min-w-0 truncate";
  label.textContent = text;
  chip.append(lead, label);
  return chip;
}

/** Builds (or rebuilds) the DOM for one chip. */
function buildChip(part: ChipPart, handlers: AttachmentChipHandlers): HTMLElement {
  if (part.type === "attachment") return buildAttachmentChip(part, handlers);
  if (part.type === "mention") {
    const glyph = iconForPath(part.name, part.path);
    const icon = chipIcon(`grid shrink-0 place-items-center ${glyph.class}`, fileIconSvg(glyph, "size-3.5 shrink-0"));
    // No `@`: the file glyph already says this is a path, and the prefix was
    // display-only — the dataset, `readParts` and the wire format never had it.
    return chipShell({ mentionPath: part.path, mentionName: part.name }, CHIP_CLASS, part.path, icon, part.name);
  }
  // A skill carries the protocol `name` in the dataset and shows the friendlier
  // `label` (a plugin's `interface.displayName` when it has one), since
  // `browser-use:browser` is not what the user picked by eye.
  const icon = chipIcon("grid shrink-0 place-items-center text-tertiary-500", SKILL_ICON);
  return chipShell(
    { skillName: part.name, skillPath: part.path, skillLabel: part.label },
    CHIP_CLASS,
    part.name,
    icon,
    part.label,
  );
}

function buildAttachmentChip(part: AttachmentPart, handlers: AttachmentChipHandlers): HTMLElement {
  const failed = part.state === "failed";
  const staging = part.state === "staging";
  const lead = document.createElement("span");
  lead.className = "grid size-4 shrink-0 place-items-center overflow-hidden rounded";
  const thumb = part.kind === "image" && !failed ? handlers.thumbSrc?.(part) : null;
  if (thumb) {
    const image = document.createElement("img");
    image.src = thumb;
    image.alt = part.filename;
    image.className = "size-4 rounded object-cover";
    lead.append(image);
  } else if (staging) {
    lead.textContent = "⏳";
    lead.className += " animate-pulse";
  } else {
    const glyph = fileIconFor(part.filename);
    lead.className += ` ${failed ? "text-error-500" : glyph.class}`;
    lead.innerHTML = fileIconSvg(glyph, "size-3.5 shrink-0");
  }
  const chip = chipShell(
    { attachmentId: part.id, attachment: JSON.stringify({ ...part, type: "attachment" }) },
    failed ? `${CHIP_SHAPE} bg-error-500/15 text-error-700-300` : CHIP_CLASS,
    failed ? `Failed to attach ${part.filename}` : part.filename,
    lead,
    part.filename,
  );

  if (part.state === "ready") {
    const meta = document.createElement("span");
    meta.className = "shrink-0 text-[10px] text-surface-500";
    meta.textContent = formatBytes(part.size);
    chip.append(meta);
  }

  if (failed) {
    const retry = document.createElement("button");
    retry.type = "button";
    retry.textContent = "Retry";
    retry.className = "shrink-0 rounded px-1 text-[10px] font-medium underline hover:no-underline";
    retry.addEventListener("click", () => handlers.onRetry(part.id));
    chip.append(retry);
  }

  return chip;
}

/**
 * Surrounds a chip with empty text nodes and returns the trailing one. A chip
 * is `contenteditable=false`, so without these the caret has no position of its
 * own beside it — it lands on the chip's box edge and paints over the pill.
 */
function padChip(chip: HTMLElement): Text {
  chip.before(document.createTextNode(""));
  const tail = document.createTextNode("");
  chip.after(tail);
  return tail;
}

/** Collapses the selection into a text node at the given offset. */
function placeCaretIn(node: Text, offset = 0): void {
  const range = document.createRange();
  range.setStart(node, offset);
  selectCollapsed(range);
}

/**
 * Replaces `range` (an active `@`/`$` query, or the caret) with a
 * non-editable chip and places the caret after it.
 */
export function insertChip(range: Range | null, part: ChipPart, handlers: AttachmentChipHandlers): void {
  const chip = buildChip(part, handlers);
  if (range) {
    range.deleteContents();
    range.insertNode(chip);
  }
  placeCaretIn(padChip(chip));
}

/**
 * Finds an attachment chip by id and rebuilds it in place with a new part
 * (e.g. staging → ready/failed), leaving the surrounding text untouched.
 */
export function updateAttachmentChip(
  root: HTMLElement,
  id: string,
  part: AttachmentPart,
  handlers: AttachmentChipHandlers,
): void {
  const existing = root.querySelector<HTMLElement>(`[data-attachment-id="${CSS.escape(id)}"]`);
  if (!existing) return;
  existing.replaceWith(buildChip(part, handlers));
}

/** Rebuilds the contenteditable DOM from composer parts (e.g. a restored draft). */
export function renderParts(root: HTMLElement, parts: ComposerPart[], handlers: AttachmentChipHandlers): void {
  root.replaceChildren();
  for (const part of parts) {
    if (part.type !== "text") {
      const chip = buildChip(part, handlers);
      root.append(chip);
      padChip(chip);
      continue;
    }
    const lines = part.text.split("\n");
    lines.forEach((line, index) => {
      if (index > 0) root.append(document.createElement("br"));
      if (line) root.append(document.createTextNode(line));
    });
  }
  // A trailing break needs a filler `<br>` after it, or the empty last line is
  // neither visible nor read back (see `isFillerBreak`).
  if (root.lastChild?.nodeName === "BR") root.append(document.createElement("br"));
}

/** Removes a chip (any kind) and restores the caret to its former position. */
export function removeChip(chip: HTMLElement): void {
  const parent = chip.parentNode;
  const index = parent ? [...parent.childNodes].indexOf(chip) : 0;
  chip.remove();
  if (!parent) return;
  const range = document.createRange();
  range.setStart(parent, index);
  selectCollapsed(range);
}

const isEmptyText = (node: Node): boolean => node.nodeType === Node.TEXT_NODE && !node.textContent;

/** Sibling in the given direction, skipping the empty text nodes that pad chips. */
function solidSibling(node: Node, direction: "back" | "forward"): Node | null {
  let current = direction === "back" ? node.previousSibling : node.nextSibling;
  while (current && isEmptyText(current)) {
    current = direction === "back" ? current.previousSibling : current.nextSibling;
  }
  return current;
}

/** The chip a node sits inside (below `root`, when given), if the caret has
 *  landed within one's label. */
function chipContaining(node: Node, root?: HTMLElement): HTMLElement | null {
  let current: Node | null = node;
  while (current && current !== root) {
    const chip = asChip(current);
    if (chip) return chip;
    current = current.parentNode;
  }
  return null;
}

/**
 * Returns the chip immediately beside a collapsed caret in the given
 * direction, if any. Used to intercept Arrow/Backspace/Delete keystrokes:
 * WebKit refuses to move the caret across (or delete) contenteditable=false
 * inline elements, so the composer has to handle them itself.
 */
export function chipBesideCaret(direction: "back" | "forward", range: Range): HTMLElement | null {
  if (!range.collapsed) return null;
  // Caret stranded inside a chip's label (WebKit does this after deleting the
  // text right after a chip): the chip itself is what's beside the caret.
  return chipContaining(range.startContainer) ?? asChip(nodeBesideCaret(direction, range));
}

/**
 * The node immediately beside a collapsed caret in the given direction,
 * skipping the empty text nodes that pad chips — or null when the caret is
 * mid-text, with characters still to cross on that side.
 */
function nodeBesideCaret(direction: "back" | "forward", range: Range): Node | null {
  const container = range.startContainer;
  if (container.nodeType === Node.TEXT_NODE) {
    const atEdge =
      direction === "back" ? range.startOffset === 0 : range.startOffset === (container.textContent?.length ?? 0);
    return atEdge ? solidSibling(container, direction) : null;
  }
  const candidate = container.childNodes[direction === "back" ? range.startOffset - 1 : range.startOffset] ?? null;
  return candidate && isEmptyText(candidate) ? solidSibling(candidate, direction) : candidate;
}

/**
 * The chip that starts (moving forward) or ends (moving backward) the line on
 * the far side of a line break next to a collapsed caret, if any.
 *
 * WebKit refuses to move the caret across a `<br>` when the only thing on the
 * other side is a `contenteditable=false` chip with no text of its own — e.g.
 * a chip placed on its own line by a Shift+Enter typed just before it. Plain
 * ArrowLeft/ArrowRight then does nothing at all, so the composer has to cross
 * the break itself.
 */
export function chipAcrossLineBreak(direction: "back" | "forward", range: Range): HTMLElement | null {
  if (!range.collapsed) return null;
  const candidate = nodeBesideCaret(direction, range);
  if (candidate?.nodeName !== "BR") return null;
  return asChip(solidSibling(candidate, direction));
}

/**
 * Whether the caret sits on the first/last *visual* line of `root`, so a
 * soft-wrapped paragraph is not mistaken for a single line. Compares the
 * caret's rect with the line boxes of everything before (or after) it.
 * Without layout (jsdom) or a caret rect it defers to the caller's logical
 * line check by answering `true`.
 */
export function caretOnVisualEdgeLine(root: HTMLElement, edge: "first" | "last"): boolean {
  const caret = liveRange();
  if (!caret) return true;
  if (typeof caret.getClientRects !== "function") return true;
  const caretRect = caret.getClientRects()[0];
  if (!caretRect || caretRect.height === 0) return true;
  const span = document.createRange();
  span.selectNodeContents(root);
  if (edge === "first") span.setEnd(caret.startContainer, caret.startOffset);
  else span.setStart(caret.startContainer, caret.startOffset);
  const rects = [...span.getClientRects()].filter((rect) => rect.height > 0);
  if (rects.length === 0) return true;
  const tolerance = caretRect.height / 2;
  return edge === "first"
    ? caretRect.top - Math.min(...rects.map((rect) => rect.top)) < tolerance
    : Math.max(...rects.map((rect) => rect.bottom)) - caretRect.bottom < tolerance;
}

/**
 * Moves the caret one visual line up or down (ArrowUp/ArrowDown), falling
 * back to the composer's own line model when WebKit's native line motion
 * refuses to move the caret at all — which it does whenever the line on the
 * far side holds only a chip, from *any* column in the current line, not
 * just its very end. Always the caller's sole driver of the move: returns
 * `true` (and has already applied the new selection) whenever it did
 * anything, `false` when there was truly nowhere to go (already the first/
 * last line) or vertical `Selection.modify` isn't available (e.g. jsdom), in
 * which case the caller should leave the keystroke alone.
 */
export function moveCaretVertically(root: HTMLElement, direction: "up" | "down"): boolean {
  const selection = window.getSelection();
  if (!selection?.rangeCount || !root.contains(selection.getRangeAt(0).startContainer)) return false;
  if (typeof selection.modify !== "function") return false;
  const before = selection.getRangeAt(0);
  const beforeContainer = before.startContainer;
  const beforeOffset = before.startOffset;
  selection.modify("move", direction === "up" ? "backward" : "forward", "line");
  const after = selection.getRangeAt(0);
  if (after.startContainer !== beforeContainer || after.startOffset !== beforeOffset) return true;

  // Stuck: land the caret ourselves, using character position within the
  // line as a (imperfect but always-successful) stand-in for visual column —
  // a chip counts as one unit, same as everywhere else in this file.
  const caret = caretOffset(root);
  if (caret === null) return false;
  const units = toUnits(readParts(root));
  const lineStart = lineEdgeOffset(units, caret, "back");
  const lineEnd = lineEdgeOffset(units, caret, "forward");
  const column = caret - lineStart;

  let targetStart: number;
  let targetEnd: number;
  if (direction === "up") {
    if (lineStart === 0) return false;
    targetEnd = lineStart - 1;
    targetStart = lineEdgeOffset(units, targetEnd, "back");
  } else {
    if (lineEnd === units.length) return false;
    targetStart = lineEnd + 1;
    targetEnd = lineEdgeOffset(units, targetStart, "forward");
  }
  placeCaretAtOffset(root, Math.min(targetStart + column, targetEnd));
  return true;
}

/** Collapses the selection to just before or just after a node (a chip, say). */
export function placeCaretBeside(node: Node, side: "before" | "after"): void {
  const range = document.createRange();
  if (side === "before") range.setStartBefore(node);
  else range.setStartAfter(node);
  selectCollapsed(range);
}

/**
 * The caret's position as an offset into the flattened content — one unit per
 * character, per line break, and per chip — or null when the caret is missing,
 * not collapsed, or outside `root`. Mirrors `readParts`' notion of a line
 * break exactly, so the two always agree on where a newline sits.
 */
export function caretOffset(root: HTMLElement): number | null {
  const range = caretRangeIn(root);
  if (!range) return null;
  let { startContainer: container, startOffset: offset } = range;
  // WebKit parks the caret inside a chip's label once the text after the chip
  // is deleted (it drops the padding text node with it). Count that as "just
  // after the chip" — the position the caret visually occupies — rather than
  // failing to find it at all.
  const inside = chipContaining(container, root);
  if (inside?.parentNode) {
    const atChipStart = container === inside && offset === 0;
    container = inside.parentNode;
    offset = [...container.childNodes].indexOf(inside) + (atChipStart ? 0 : 1);
  }

  let total = 0;
  let found: number | null = null;
  const visit = (node: Node): void => {
    if (found !== null) return;
    if (node.nodeType === Node.TEXT_NODE) {
      const length = node.textContent?.length ?? 0;
      if (node === container) found = total + Math.min(offset, length);
      else total += length;
      return;
    }
    if (asChip(node)) {
      total += 1;
      return;
    }
    if (node.nodeName === "BR") {
      if (!isFillerBreak(node)) total += 1;
      return;
    }
    let previous: Node | null = null;
    const children = [...node.childNodes];
    const limit = node === container ? Math.min(offset, children.length) : children.length;
    for (let index = 0; index < limit; index += 1) {
      const child = children[index];
      // Empty text nodes carry no offset of their own, but the caret does sit
      // in them — they are what pads every chip — so never skip the container.
      if (isEmptyText(child) && child !== container) continue;
      if (previous && (isBlock(child) || isBlock(previous))) total += 1;
      visit(child);
      if (found !== null) return;
      previous = child;
    }
    if (node === container) found = total;
  };
  visit(root);
  return found;
}

/** Places the caret at a flattened offset in freshly rendered content. */
export function placeCaretAtOffset(root: HTMLElement, target: number): void {
  const place = (): void => {
    let remaining = target;
    for (const node of root.childNodes) {
      if (node.nodeType === Node.TEXT_NODE) {
        const length = node.textContent?.length ?? 0;
        if (remaining <= length) {
          placeCaretIn(node as Text, remaining);
          return;
        }
        remaining -= length;
      } else {
        if (remaining === 0) {
          placeCaretBeside(node, "before");
          return;
        }
        remaining -= 1;
      }
    }
    placeCaretAtEnd(root);
  };
  place();
  scrollCaretIntoView(root);
}

/** Collapses the selection to the very end of `root`. */
export function placeCaretAtEnd(root: HTMLElement): void {
  const range = document.createRange();
  range.selectNodeContents(root);
  selectCollapsed(range, false);
}

/**
 * Scrolls the editor so the caret is visible again. Re-rendering replaces the
 * root's children, which resets its scrollTop to 0 — without this, every
 * Shift+Enter, paste or undo in a scrolled composer jumps the view to the top.
 */
function scrollCaretIntoView(root: HTMLElement): void {
  const range = caretRangeIn(root);
  if (!range) return;
  // No Range layout (e.g. jsdom): nothing is scrollable there either.
  if (typeof range.getClientRects !== "function") return;
  let rect: DOMRect | null = range.getClientRects()[0] ?? null;
  if (!rect || rect.height === 0) {
    // A caret in an empty text node (chip padding) or after a trailing <br>
    // has no box of its own; measure a probe character there instead. The
    // probe is emptied rather than removed so the caret can stay in it —
    // empty text nodes are already part of this editor's normal shape.
    const probe = document.createTextNode("​");
    range.insertNode(probe);
    const probeRange = document.createRange();
    probeRange.selectNodeContents(probe);
    rect = probeRange.getBoundingClientRect();
    probe.data = "";
    placeCaretIn(probe);
  }
  if (!rect) return;
  const box = root.getBoundingClientRect();
  if (rect.bottom > box.bottom) root.scrollTop += rect.bottom - box.bottom;
  else if (rect.top < box.top) root.scrollTop -= box.top - rect.top;
}

/** Explodes parts into one entry per caret unit: a character or a whole chip. */
function toUnits(parts: ComposerPart[]): Units {
  const units: Units = [];
  for (const part of parts) {
    if (part.type === "text") units.push(...part.text);
    else units.push(part);
  }
  return units;
}

function fromUnits(units: Units): ComposerPart[] {
  return normaliseParts(units.map((unit) => (typeof unit === "string" ? { type: "text", text: unit } : unit)));
}

/**
 * The frame every parts-model edit shares: read the caret and the content as
 * units, let `edit` splice them in place and return where the caret goes —
 * or null to leave the keystroke to the browser — then re-render and return
 * the new parts.
 */
function editUnits(
  root: HTMLElement,
  handlers: AttachmentChipHandlers,
  edit: (units: Units, caret: number) => number | null,
): ComposerPart[] | null {
  const caret = caretOffset(root);
  if (caret === null) return null;
  const units = toUnits(readParts(root));
  const nextCaret = edit(units, caret);
  if (nextCaret === null) return null;
  const parts = fromUnits(units);
  renderParts(root, parts, handlers);
  placeCaretAtOffset(root, nextCaret);
  return parts;
}

/** Offset of the start/end of the line `caret` sits on, in flattened units. */
function lineEdgeOffset(units: Units, caret: number, direction: "back" | "forward"): number {
  let edge = caret;
  if (direction === "back") while (edge > 0 && units[edge - 1] !== "\n") edge -= 1;
  else while (edge < units.length && units[edge] !== "\n") edge += 1;
  return edge;
}

const isSpaceUnit = (unit: Unit) => typeof unit === "string" && unit !== "\n" && /\s/.test(unit);
const isWordUnit = (unit: Unit) => typeof unit === "string" && unit !== "\n" && !/\s/.test(unit);

/**
 * Offset of the far side of the "word" beside the caret, in flattened units,
 * plus whether reaching it crossed a chip.
 *
 * A chip counts as one whole word, consumed together with any whitespace
 * skipped to reach it — matching how word motion already treats a run of
 * whitespace-then-word as a single step.
 */
function wordEdgeOffset(
  units: Units,
  caret: number,
  direction: "back" | "forward",
): { offset: number; crossesChip: boolean } {
  let edge = caret;
  let crossesChip = false;
  if (direction === "back") {
    while (edge > 0 && isSpaceUnit(units[edge - 1])) edge -= 1;
    if (edge > 0 && typeof units[edge - 1] !== "string") {
      edge -= 1;
      crossesChip = true;
    } else {
      while (edge > 0 && isWordUnit(units[edge - 1])) edge -= 1;
    }
  } else {
    while (edge < units.length && isSpaceUnit(units[edge])) edge += 1;
    if (edge < units.length && typeof units[edge] !== "string") {
      edge += 1;
      crossesChip = true;
    } else {
      while (edge < units.length && isWordUnit(units[edge])) edge += 1;
    }
  }
  return { offset: edge, crossesChip };
}

/**
 * Deletes the single line break beside the caret and re-renders the editor,
 * returning the new parts — or null when the caret is not next to a break, in
 * which case the caller lets the browser delete the character itself.
 *
 * Engines disagree about what Backspace means at a line boundary: WebKit merges
 * the surrounding blocks and, when its own `<div>` lines sit next to the
 * composer's `<br>`s, swallows two breaks at once. Doing the edit against the
 * parts model keeps one keystroke to exactly one break.
 */
export function deleteLineBreak(
  root: HTMLElement,
  direction: "back" | "forward",
  handlers: AttachmentChipHandlers,
): ComposerPart[] | null {
  return editUnits(root, handlers, (units, caret) => {
    const index = direction === "back" ? caret - 1 : caret;
    if (units[index] !== "\n") return null;
    units.splice(index, 1);
    return index;
  });
}

/**
 * Inserts a single line break at the caret and re-renders, returning the new
 * parts — or null when there is no caret to work from, leaving the keystroke to
 * the browser.
 *
 * Shift+Enter is intercepted for the same reason Backspace is. Asked for a
 * break next to a `contenteditable=false` chip, WebKit writes *two* `<br>`s —
 * the chip drops two lines instead of one, and the caret is left on the dead
 * line between them, which it then cannot leave: moving right would have to
 * cross the chip, and WebKit will not move a caret across one.
 */
export function insertLineBreak(root: HTMLElement, handlers: AttachmentChipHandlers): ComposerPart[] | null {
  return editUnits(root, handlers, (units, caret) => {
    units.splice(caret, 0, "\n");
    return caret + 1;
  });
}

/**
 * Deletes everything between the caret and the start/end of its line (Cmd+
 * Backspace / Cmd+Delete), chips included, and returns the new parts — or null
 * when there is nothing on that side of the caret to take.
 *
 * Like `deleteLineBreak`, this runs against the parts model: WebKit's own
 * delete-to-line-start stalls at a `contenteditable=false` chip and merges its
 * block lines with the composer's `<br>`s, so one keystroke can take a chip
 * only, or a line and a half.
 */
export function deleteToLineEdge(
  root: HTMLElement,
  direction: "back" | "forward",
  handlers: AttachmentChipHandlers,
): ComposerPart[] | null {
  return editUnits(root, handlers, (units, caret) => {
    const edge = lineEdgeOffset(units, caret, direction);
    return spliceBetween(units, caret, edge);
  });
}

/** Removes the units between two offsets (in either order), returning the
 *  lower one — or null when they are the same and nothing was removed. */
function spliceBetween(units: Units, a: number, b: number): number | null {
  const start = Math.min(a, b);
  const end = Math.max(a, b);
  if (start === end) return null;
  units.splice(start, end - start);
  return start;
}

/**
 * Deletes the single "word" beside the caret (Option+Backspace / Option+
 * Delete) against the parts model, but only when a chip is part of what
 * would be taken — returns null otherwise, leaving plain text word-deletion
 * to the browser, which handles it fine on its own.
 *
 * A chip counts as one whole word, consumed together with any whitespace
 * skipped to reach it (matching how word-delete already treats a run of
 * whitespace-then-word as a single step). WebKit's native word motion can't
 * cross into a chip cleanly: deleting a lone space next to one has been seen
 * to leave behind a stray `<br>` and strand the caret at the very start of
 * the composer instead of where the deletion happened.
 */
export function deleteToWordEdge(
  root: HTMLElement,
  direction: "back" | "forward",
  handlers: AttachmentChipHandlers,
): ComposerPart[] | null {
  return editUnits(root, handlers, (units, caret) => {
    const { offset, crossesChip } = wordEdgeOffset(units, caret, direction);
    return crossesChip ? spliceBetween(units, caret, offset) : null;
  });
}

/**
 * Deletes the single plain character beside the caret when removing it would
 * put the caret at a chip's edge, against the parts model — or null when
 * that's not the situation, leaving the keystroke to the browser.
 *
 * Deleting that one character natively is what strands the composer: WebKit
 * parks the resulting caret inside the chip's own label rather than in the
 * empty text node that pads it (see `chipBesideCaret` and `caretOffset`
 * above), and can drop that padding node entirely — breaking the flat,
 * padded-chip shape every caret helper here assumes, sometimes for good.
 * Handling the deletion here, the same way `deleteLineBreak` and
 * `deleteToWordEdge` handle their own browser-hostile cases, keeps the chip
 * flat and padded and the caret exactly where the parts model puts it.
 */
export function deleteCharBesideChip(
  root: HTMLElement,
  direction: "back" | "forward",
  handlers: AttachmentChipHandlers,
): ComposerPart[] | null {
  return editUnits(root, handlers, (units, caret) => {
    const charIndex = direction === "back" ? caret - 1 : caret;
    const chipIndex = direction === "back" ? caret - 2 : caret + 1;
    const char = units[charIndex];
    const chip = units[chipIndex];
    if (typeof char !== "string" || char === "\n") return null;
    if (chip === undefined || typeof chip === "string") return null;
    units.splice(charIndex, 1);
    return charIndex;
  });
}

/** Whether a chip is missing the empty text node that pads it on either side. */
const isUnpadded = (chip: Element): boolean =>
  !(chip.previousSibling && isEmptyText(chip.previousSibling)) || !(chip.nextSibling && isEmptyText(chip.nextSibling));

/**
 * Whether the browser has rewritten the editor out of the flat shape the caret
 * helpers here assume: only text, `<br>`s and chips as direct children of the
 * root, each chip padded by empty text nodes.
 */
function needsFlattening(root: HTMLElement): boolean {
  if (root.querySelector("div,p")) return true;
  const container = liveRange()?.startContainer;
  if (container && root.contains(container) && chipContaining(container, root)) return true;
  return [...root.querySelectorAll(CHIP_SELECTOR)].some((chip) => chip.parentNode !== root || isUnpadded(chip));
}

/**
 * Re-flattens the editor after the browser has rewritten it, returning the new
 * parts — or null when nothing needed normalising.
 *
 * Engines answer Enter, Cmd+Backspace and paste with `<div>` line blocks, and
 * drop the empty text nodes that pad chips. Both break the flat DOM that
 * `chipBesideCaret` and friends rely on — a chip in a block of its own is not a
 * sibling of the caret's text node, so the Arrow/Backspace interception misses
 * it and WebKit, which will not move a caret across a contenteditable=false
 * element, leaves it stuck. Re-rendering from the parts model restores both the
 * flat shape and the padding, keeping the caret where it was.
 */
export function normaliseEditorDom(root: HTMLElement, handlers: AttachmentChipHandlers): ComposerPart[] | null {
  if (!needsFlattening(root)) return null;
  // Without a caret to restore, re-rendering would move it somewhere arbitrary,
  // so `editUnits` leaves the DOM alone and a later edit normalises instead.
  return editUnits(root, handlers, (_units, caret) => caret);
}

/** Where an edge-seeking motion should land, as a flattened unit offset. */
type EdgeOffset = (units: Units, caret: number, direction: "back" | "forward") => number;

/**
 * Moves the caret to an edge computed from the parts model, ignoring the
 * browser entirely. Returns whether it moved. Only hard line breaks are known
 * here, so this is a fallback for when native motion refuses to move at all —
 * not a replacement for it, which would lose soft-wrap awareness.
 */
function moveCaretByModel(root: HTMLElement, direction: "back" | "forward", edgeOffset: EdgeOffset): boolean {
  const caret = caretOffset(root);
  if (caret === null) return false;
  const target = edgeOffset(toUnits(readParts(root)), caret, direction);
  if (target === caret) return false;
  placeCaretAtOffset(root, target);
  return true;
}

/**
 * Moves the caret to the edge of the current line or word, crossing any chip
 * the native motion stalls at.
 *
 * WebKit will not move a caret across a `contenteditable=false` element, so
 * `Selection.modify` either parks the caret beside the chip or drops it inside
 * the chip's own label — and sometimes refuses to move at all. Each hop
 * therefore checks both, steps the caret out past the chip by hand, and asks
 * for the motion again; when nothing at all worked, the parts model has the
 * final say so a `preventDefault`ed keystroke is never a no-op.
 */
function moveCaretToEdge(
  editor: HTMLElement,
  direction: "back" | "forward",
  granularity: "lineboundary" | "word",
  edgeOffset: EdgeOffset,
): void {
  const selection = window.getSelection();
  if (!selection?.rangeCount || !editor.contains(selection.getRangeAt(0).startContainer)) return;
  if (typeof selection.modify !== "function") {
    // No Selection.modify (e.g. jsdom): the parts model is all there is.
    moveCaretByModel(editor, direction, edgeOffset);
    return;
  }
  const side = direction === "back" ? "before" : "after";
  const start = selection.getRangeAt(0);
  const startNode = start.startContainer;
  const startOffset = start.startOffset;
  let previousNode = startNode;
  let previousOffset = startOffset;

  for (let hops = 0; hops < 50; hops += 1) {
    selection.modify("move", direction === "back" ? "backward" : "forward", granularity);
    let range = selection.getRangeAt(0);
    // Landed inside a chip's label: step out to the far side of the chip.
    const inside = chipContaining(range.startContainer, editor);
    if (inside) {
      placeCaretBeside(inside, side);
      range = selection.getRangeAt(0);
    }
    const beside = chipBesideCaret(direction, range);
    if (beside) placeCaretBeside(beside, side);

    const now = selection.getRangeAt(0);
    const stuck = now.startContainer === previousNode && now.startOffset === previousOffset;
    previousNode = now.startContainer;
    previousOffset = now.startOffset;
    if (stuck) break;
    // No chip in the way means the native motion reached the edge cleanly.
    if (!inside && !beside) break;
  }

  if (previousNode === startNode && previousOffset === startOffset) {
    moveCaretByModel(editor, direction, edgeOffset);
  }
}

/**
 * Moves the caret to the start/end of the current visual line (Cmd+Arrow on
 * macOS), hopping over any chip the native lineboundary motion stalls at.
 */
export function moveCaretToLineEdge(editor: HTMLElement, direction: "back" | "forward"): void {
  moveCaretToEdge(editor, direction, "lineboundary", lineEdgeOffset);
}

/**
 * Moves the caret one word left/right (Option+Arrow on macOS), treating each
 * chip as a single word.
 *
 * WebKit's own word motion does not stall at a chip so much as look straight
 * through it: from just before one, Option+Right lands past the *next* real
 * word, leaving no way to stop beside the chip. So when the model says a chip
 * is what the step would cross, the model wins — which is also what
 * `deleteToWordEdge` does, keeping Option+Right and Option+Backspace agreed on
 * where a word ends. Everywhere else the browser's own word rules (punctuation,
 * non-Latin scripts) beat anything reimplemented here.
 */
export function moveCaretToWordEdge(editor: HTMLElement, direction: "back" | "forward"): void {
  const caret = caretOffset(editor);
  if (caret !== null) {
    const { offset, crossesChip } = wordEdgeOffset(toUnits(readParts(editor)), caret, direction);
    if (crossesChip && offset !== caret) {
      placeCaretAtOffset(editor, offset);
      return;
    }
  }
  moveCaretToEdge(editor, direction, "word", (units, at, dir) => wordEdgeOffset(units, at, dir).offset);
}

import { detectSlashQuery } from "$lib/composer/slashCommands";
import type { Mention, UserInputPart } from "$lib/types";
import { fileIconFor, fileIconSvg, iconForPath } from "$lib/utils/fileIcons";
import { relativeMentionPath } from "$lib/utils/mentions";

/** Progress of a staged attachment: mid-copy, ready to send, or copy failed. */
export type AttachmentState = "staging" | "ready" | "failed";

/** A file or image the user attached, rendered as an inline chip at the caret. */
export interface AttachmentPart {
  type: "attachment";
  id: string;
  filename: string;
  mime: string;
  size: number;
  /** Staged copy path (empty while staging/failed). */
  path: string;
  kind: "image" | "file";
  state: AttachmentState;
}

export type ComposerPart =
  | { type: "text"; text: string }
  | { type: "mention"; name: string; path: string }
  | { type: "skill"; name: string; label: string }
  | AttachmentPart;

/** Callbacks wired into every attachment chip's controls. */
export interface AttachmentChipHandlers {
  onRemove: (chip: HTMLElement) => void;
  onRetry: (id: string) => void;
  /** Resolves an image chip's thumbnail src (asset URL / blob), or null. */
  thumbSrc?: (part: AttachmentPart) => string | null;
}

/** Human-readable file size for chip labels (e.g. "12 KB", "3.4 MB"). */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function normaliseParts(input: ComposerPart[]): ComposerPart[] {
  const normalised: ComposerPart[] = [];
  for (const part of input) {
    if (part.type === "text") {
      if (!part.text) continue;
      const previous = normalised.at(-1);
      if (previous?.type === "text") previous.text += part.text;
      else normalised.push(part);
    } else {
      normalised.push(part);
    }
  }
  return normalised.length > 0 ? normalised : [{ type: "text", text: "" }];
}

const isBlock = (node: Node): boolean => /^(DIV|P)$/.test(node.nodeName);

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
    if (node instanceof HTMLElement && node.dataset.attachmentId) {
      const attachment = readAttachmentDataset(node);
      if (attachment) return [attachment];
    }
    if (node instanceof HTMLElement && node.dataset.mentionPath) {
      return [{ type: "mention", name: node.dataset.mentionName ?? "file", path: node.dataset.mentionPath }];
    }
    if (node instanceof HTMLElement && node.dataset.skillName) {
      const name = node.dataset.skillName;
      return [{ type: "skill", name, label: node.dataset.skillLabel ?? name }];
    }
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
 * Turns composer parts into the `turn/start` input array. Images that finished
 * staging become native `localImage` items; non-image files are appended as a
 * clearly-labelled text reference (the protocol has no native file item), in
 * their original position. Attachments still staging or failed are dropped.
 *
 * File mentions go out as cwd-relative markdown links, the form Codex itself
 * writes and round-trips. The protocol's `mention` item looks like a fit but is
 * not one: its `path` is a connector/plugin target (`app://…`, `plugin://…`), so
 * a filesystem path resolves to nothing and is dropped without an error.
 *
 * Skills, by contrast, do have a native item — `{type:"skill", name}` — so they
 * go out as themselves rather than as text.
 */
export function buildTurnInput(parts: ComposerPart[], cwd = ""): UserInputPart[] {
  const input: UserInputPart[] = [];
  for (const part of parts) {
    if (part.type === "text") {
      input.push({ type: "text", text: part.text });
    } else if (part.type === "mention") {
      input.push({ type: "text", text: `[${part.name}](${relativeMentionPath(part.path, cwd)})` });
    } else if (part.type === "skill") {
      input.push({ type: "skill", name: part.name });
    } else if (part.type === "attachment" && part.state === "ready") {
      if (part.kind === "image") {
        input.push({ type: "localImage", path: part.path });
      } else {
        input.push({ type: "text", text: `\n[Attached file: ${part.filename} — ${part.path}]\n` });
      }
    }
  }
  return input.length > 0 ? input : [{ type: "text", text: "" }];
}

/** Whether any part carries sendable content (text or a ready attachment). */
export function hasSendableContent(parts: ComposerPart[]): boolean {
  return parts.some(
    (part) =>
      (part.type === "text" && part.text.trim().length > 0) ||
      part.type === "mention" ||
      part.type === "skill" ||
      (part.type === "attachment" && part.state === "ready"),
  );
}

export interface DetectedQueries {
  slashQuery: string | null;
  mentionQuery: string | null;
  mentionRange: Range | null;
  skillQuery: string | null;
  skillRange: Range | null;
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
  const selection = window.getSelection();
  if (!selection?.rangeCount) return null;
  const range = selection.getRangeAt(0);
  if (!range.collapsed || !root.contains(range.startContainer)) return null;
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
  const slashQuery = detectSlashQuery(before);
  const empty = { slashQuery, mentionQuery: null, mentionRange: null, skillQuery: null, skillRange: null };

  /** The range covering the trigger character and the query typed after it. */
  const rangeFor = (query: string): Range | null => {
    if (!textNode) return null;
    const range = document.createRange();
    range.setStart(textNode, caretOffset - query.length - 1);
    range.setEnd(textNode, caretOffset);
    return range;
  };

  const mention = before.match(/(?:^|\s)@([\w./-]*)$/);
  if (mention && cwd && textNode) {
    return { ...empty, mentionQuery: mention[1], mentionRange: rangeFor(mention[1]) };
  }
  // Skill names are namespaced (`browser-use:browser`), so `:` is part of the
  // query rather than a terminator.
  const skill = before.match(/(?:^|\s)\$([\w:.-]*)$/);
  if (skill && textNode) {
    return { ...empty, skillQuery: skill[1], skillRange: rangeFor(skill[1]) };
  }
  return empty;
}

function buildMentionChip(mention: Mention, onRemove: (chip: HTMLElement) => void): HTMLElement {
  const chip = document.createElement("span");
  chip.contentEditable = "false";
  chip.dataset.mentionPath = mention.path;
  chip.dataset.mentionName = mention.name;
  chip.className =
    "group/chip mx-1 inline-flex select-none items-center gap-1.5 rounded-full bg-primary-500/15 py-1 pl-2 pr-1.5 align-baseline text-xs font-medium text-primary-700-300 transition-all duration-150 hover:bg-primary-500/25 hover:shadow-sm";
  chip.title = mention.path;
  const glyph = iconForPath(mention.name, mention.path);
  const icon = document.createElement("span");
  icon.className = `grid shrink-0 place-items-center ${glyph.class}`;
  icon.innerHTML = fileIconSvg(glyph, "size-3.5 shrink-0");
  const label = document.createElement("span");
  label.textContent = `@${mention.name}`;
  const remove = document.createElement("button");
  remove.type = "button";
  remove.ariaLabel = `Remove ${mention.name}`;
  remove.className =
    "grid size-4 place-items-center rounded-full text-[10px] opacity-0 transition-opacity hover:bg-primary-500/25 focus-visible:opacity-100 group-hover/chip:opacity-100";
  remove.textContent = "×";
  remove.addEventListener("click", () => onRemove(chip));
  chip.append(icon, label, remove);
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
  range.collapse(true);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
}

/** Replaces the active mention query with a non-editable chip and places the caret after it. */
export function insertMentionChip(range: Range, mention: Mention, onRemove: (chip: HTMLElement) => void): void {
  const chip = buildMentionChip(mention, onRemove);
  range.deleteContents();
  range.insertNode(chip);
  placeCaretIn(padChip(chip));
}

/**
 * Chip for a picked skill. Carries the protocol `name` in the dataset and shows
 * the friendlier `label` (a plugin's `interface.displayName` when it has one),
 * since `browser-use:browser` is not what the user picked by eye.
 */
function buildSkillChip(name: string, label: string, onRemove: (chip: HTMLElement) => void): HTMLElement {
  const chip = document.createElement("span");
  chip.contentEditable = "false";
  chip.dataset.skillName = name;
  chip.dataset.skillLabel = label;
  chip.className =
    "group/chip mx-1 inline-flex select-none items-center gap-1.5 rounded-full bg-tertiary-500/15 py-1 pl-2 pr-1.5 align-baseline text-xs font-medium text-tertiary-700-300 transition-all duration-150 hover:bg-tertiary-500/25 hover:shadow-sm";
  chip.title = name;
  const icon = document.createElement("span");
  icon.className = "grid shrink-0 place-items-center";
  // Sparkles, matching how a skill renders in the transcript.
  icon.innerHTML =
    '<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="size-3.5 shrink-0"><path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z"/></svg>';
  const text = document.createElement("span");
  text.textContent = label;
  const remove = document.createElement("button");
  remove.type = "button";
  remove.ariaLabel = `Remove ${label}`;
  remove.className =
    "grid size-4 place-items-center rounded-full text-[10px] opacity-0 transition-opacity hover:bg-tertiary-500/25 focus-visible:opacity-100 group-hover/chip:opacity-100";
  remove.textContent = "×";
  remove.addEventListener("click", () => onRemove(chip));
  chip.append(icon, text, remove);
  return chip;
}

/** Replaces the active `$` query with a non-editable skill chip. */
export function insertSkillChip(
  range: Range,
  name: string,
  label: string,
  onRemove: (chip: HTMLElement) => void,
): void {
  const chip = buildSkillChip(name, label, onRemove);
  range.deleteContents();
  range.insertNode(chip);
  placeCaretIn(padChip(chip));
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

/** Builds (or rebuilds) the DOM for one attachment chip. */
function buildAttachmentChip(part: AttachmentPart, handlers: AttachmentChipHandlers): HTMLElement {
  const chip = document.createElement("span");
  chip.contentEditable = "false";
  chip.dataset.attachmentId = part.id;
  chip.dataset.attachment = JSON.stringify({ ...part, type: "attachment" });
  const failed = part.state === "failed";
  const staging = part.state === "staging";
  chip.className = `mx-1 inline-flex max-w-[16rem] select-none items-center gap-1.5 rounded-lg py-1 pl-1.5 pr-1 align-baseline text-xs ${
    failed ? "bg-error-500/15 text-error-700-300" : "bg-surface-200-800"
  }`;
  chip.title = failed ? `Failed to attach ${part.filename}` : part.filename;

  const lead = document.createElement("span");
  lead.className = "grid size-5 shrink-0 place-items-center overflow-hidden rounded";
  const thumb = part.kind === "image" && !failed ? handlers.thumbSrc?.(part) : null;
  if (thumb) {
    const image = document.createElement("img");
    image.src = thumb;
    image.alt = part.filename;
    image.className = "size-5 rounded object-cover";
    lead.append(image);
  } else if (staging) {
    lead.textContent = "⏳";
    lead.className += " animate-pulse";
  } else {
    const glyph = fileIconFor(part.filename);
    lead.className += ` ${failed ? "text-error-500" : glyph.class}`;
    lead.innerHTML = fileIconSvg(glyph, "size-3.5 shrink-0");
  }
  chip.append(lead);

  const label = document.createElement("span");
  label.className = "min-w-0 truncate";
  label.textContent = part.filename;
  chip.append(label);

  if (part.state === "ready") {
    const meta = document.createElement("span");
    meta.className = "shrink-0 text-[10px] text-surface-500";
    meta.textContent = formatSize(part.size);
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

  const remove = document.createElement("button");
  remove.type = "button";
  remove.ariaLabel = `Remove ${part.filename}`;
  remove.className = "grid size-4 shrink-0 place-items-center rounded-full text-[10px] hover:bg-surface-300-700";
  remove.textContent = "×";
  remove.addEventListener("click", () => handlers.onRemove(chip));
  chip.append(remove);
  return chip;
}

/** Inserts a new attachment chip at the caret, placing the caret after it. */
export function insertAttachmentChip(part: AttachmentPart, handlers: AttachmentChipHandlers): void {
  const selection = window.getSelection();
  const chip = buildAttachmentChip(part, handlers);
  const range = selection?.rangeCount ? selection.getRangeAt(0) : null;
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
  existing.replaceWith(buildAttachmentChip(part, handlers));
}

/** Rebuilds the contenteditable DOM from composer parts (e.g. a restored draft). */
export function renderParts(root: HTMLElement, parts: ComposerPart[], onRemove: (chip: HTMLElement) => void): void {
  renderPartsWith(root, parts, { onRemove, onRetry: () => {} });
}

/** Like `renderParts` but with full attachment handlers (thumbnails, retry). */
export function renderPartsWith(root: HTMLElement, parts: ComposerPart[], handlers: AttachmentChipHandlers): void {
  root.replaceChildren();
  for (const part of parts) {
    if (part.type === "attachment" || part.type === "mention" || part.type === "skill") {
      let chip: HTMLElement;
      if (part.type === "attachment") chip = buildAttachmentChip(part, handlers);
      else if (part.type === "mention") chip = buildMentionChip(part, handlers.onRemove);
      else chip = buildSkillChip(part.name, part.label, handlers.onRemove);
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

/** Removes a mention chip and restores the caret to its former position. */
export function removeMentionChip(chip: HTMLElement): void {
  const parent = chip.parentNode;
  const index = parent ? [...parent.childNodes].indexOf(chip) : 0;
  chip.remove();
  if (parent) {
    const range = document.createRange();
    range.setStart(parent, index);
    range.collapse(true);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  }
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

const asChip = (node: Node | null | undefined): HTMLElement | null =>
  node instanceof HTMLElement && (node.dataset.mentionPath || node.dataset.attachmentId) ? node : null;

/**
 * Returns the mention chip immediately beside a collapsed caret in the given
 * direction, if any. Used to intercept Arrow/Backspace/Delete keystrokes:
 * WebKit refuses to move the caret across (or delete) contenteditable=false
 * inline elements, so the composer has to handle them itself.
 */
export function chipBesideCaret(direction: "back" | "forward", range: Range): HTMLElement | null {
  if (!range.collapsed) return null;
  const container = range.startContainer;
  let candidate: Node | null;
  if (container.nodeType === Node.TEXT_NODE) {
    const atEdge =
      direction === "back" ? range.startOffset === 0 : range.startOffset === (container.textContent?.length ?? 0);
    if (!atEdge) return null;
    candidate = solidSibling(container, direction);
  } else {
    candidate = container.childNodes[direction === "back" ? range.startOffset - 1 : range.startOffset] ?? null;
    if (candidate && isEmptyText(candidate)) candidate = solidSibling(candidate, direction);
  }
  return asChip(candidate);
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
  const container = range.startContainer;
  let candidate: Node | null;
  if (container.nodeType === Node.TEXT_NODE) {
    const atEdge =
      direction === "back" ? range.startOffset === 0 : range.startOffset === (container.textContent?.length ?? 0);
    if (!atEdge) return null;
    candidate = solidSibling(container, direction);
  } else {
    candidate = container.childNodes[direction === "back" ? range.startOffset - 1 : range.startOffset] ?? null;
    if (candidate && isEmptyText(candidate)) candidate = solidSibling(candidate, direction);
  }
  if (candidate?.nodeName !== "BR") return null;
  return asChip(solidSibling(candidate, direction));
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
  let lineStart = caret;
  while (lineStart > 0 && units[lineStart - 1] !== "\n") lineStart -= 1;
  let lineEnd = caret;
  while (lineEnd < units.length && units[lineEnd] !== "\n") lineEnd += 1;
  const column = caret - lineStart;

  let targetStart: number;
  let targetEnd: number;
  if (direction === "up") {
    if (lineStart === 0) return false;
    const prevEnd = lineStart - 1;
    let prevStart = prevEnd;
    while (prevStart > 0 && units[prevStart - 1] !== "\n") prevStart -= 1;
    targetStart = prevStart;
    targetEnd = prevEnd;
  } else {
    if (lineEnd === units.length) return false;
    const nextStart = lineEnd + 1;
    let nextEnd = nextStart;
    while (nextEnd < units.length && units[nextEnd] !== "\n") nextEnd += 1;
    targetStart = nextStart;
    targetEnd = nextEnd;
  }
  placeCaretAtOffset(root, Math.min(targetStart + column, targetEnd));
  return true;
}

/** Collapses the selection to just before or just after a node. */
function placeCaretBesideNode(node: Node, side: "before" | "after"): void {
  const range = document.createRange();
  if (side === "before") range.setStartBefore(node);
  else range.setStartAfter(node);
  range.collapse(true);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
}

/** Collapses the selection to just before or just after a chip. */
export function placeCaretBesideChip(chip: HTMLElement, side: "before" | "after"): void {
  placeCaretBesideNode(chip, side);
}

/**
 * The caret's position as an offset into the flattened content — one unit per
 * character, per line break, and per chip — or null when the caret is missing,
 * not collapsed, or outside `root`. Mirrors `readParts`' notion of a line
 * break exactly, so the two always agree on where a newline sits.
 */
export function caretOffset(root: HTMLElement): number | null {
  const selection = window.getSelection();
  if (!selection?.rangeCount) return null;
  const range = selection.getRangeAt(0);
  if (!range.collapsed || !root.contains(range.startContainer)) return null;
  const { startContainer: container, startOffset: offset } = range;

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
    if (node instanceof HTMLElement && (node.dataset.mentionPath || node.dataset.attachmentId)) {
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
      if (isEmptyText(child)) continue;
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
          placeCaretBesideNode(node, "before");
          return;
        }
        remaining -= 1;
      }
    }
    const range = document.createRange();
    range.selectNodeContents(root);
    range.collapse(false);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
  };
  place();
  scrollCaretIntoView(root);
}

/**
 * Scrolls the editor so the caret is visible again. Re-rendering replaces the
 * root's children, which resets its scrollTop to 0 — without this, every
 * Shift+Enter, paste or undo in a scrolled composer jumps the view to the top.
 */
function scrollCaretIntoView(root: HTMLElement): void {
  const selection = window.getSelection();
  if (!selection?.rangeCount) return;
  const range = selection.getRangeAt(0);
  if (!range.collapsed || !root.contains(range.startContainer)) return;
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
function toUnits(parts: ComposerPart[]): (string | ComposerPart)[] {
  const units: (string | ComposerPart)[] = [];
  for (const part of parts) {
    if (part.type === "text") units.push(...part.text);
    else units.push(part);
  }
  return units;
}

function fromUnits(units: (string | ComposerPart)[]): ComposerPart[] {
  return normaliseParts(units.map((unit) => (typeof unit === "string" ? { type: "text", text: unit } : unit)));
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
  const caret = caretOffset(root);
  if (caret === null) return null;
  const units = toUnits(readParts(root));
  const index = direction === "back" ? caret - 1 : caret;
  if (units[index] !== "\n") return null;
  units.splice(index, 1);
  const parts = fromUnits(units);
  renderPartsWith(root, parts, handlers);
  placeCaretAtOffset(root, index);
  return parts;
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
  const caret = caretOffset(root);
  if (caret === null) return null;
  const units = toUnits(readParts(root));
  units.splice(caret, 0, "\n");
  const parts = fromUnits(units);
  renderPartsWith(root, parts, handlers);
  placeCaretAtOffset(root, caret + 1);
  return parts;
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
  const caret = caretOffset(root);
  if (caret === null) return null;
  const units = toUnits(readParts(root));
  let start = caret;
  let end = caret;
  if (direction === "back") while (start > 0 && units[start - 1] !== "\n") start -= 1;
  else while (end < units.length && units[end] !== "\n") end += 1;
  if (start === end) return null;
  units.splice(start, end - start);
  const parts = fromUnits(units);
  renderPartsWith(root, parts, handlers);
  placeCaretAtOffset(root, start);
  return parts;
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
  const caret = caretOffset(root);
  if (caret === null) return null;
  const units = toUnits(readParts(root));
  const isSpace = (unit: string | ComposerPart) => typeof unit === "string" && unit !== "\n" && /\s/.test(unit);
  const isWordChar = (unit: string | ComposerPart) => typeof unit === "string" && unit !== "\n" && !/\s/.test(unit);

  let start = caret;
  let end = caret;
  let crossesChip = false;
  if (direction === "back") {
    while (start > 0 && isSpace(units[start - 1])) start -= 1;
    if (start > 0 && typeof units[start - 1] !== "string") {
      start -= 1;
      crossesChip = true;
    } else {
      while (start > 0 && isWordChar(units[start - 1])) start -= 1;
    }
  } else {
    while (end < units.length && isSpace(units[end])) end += 1;
    if (end < units.length && typeof units[end] !== "string") {
      end += 1;
      crossesChip = true;
    } else {
      while (end < units.length && isWordChar(units[end])) end += 1;
    }
  }
  if (!crossesChip || start === end) return null;
  units.splice(start, end - start);
  const parts = fromUnits(units);
  renderPartsWith(root, parts, handlers);
  placeCaretAtOffset(root, start);
  return parts;
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
  return [...root.querySelectorAll("[data-mention-path],[data-attachment-id]")].some(
    (chip) => chip.parentNode !== root || isUnpadded(chip),
  );
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
  // Without a caret to restore, re-rendering would move it somewhere arbitrary;
  // leave the DOM alone and normalise on a later edit instead.
  const caret = caretOffset(root);
  if (caret === null) return null;
  const parts = readParts(root);
  renderPartsWith(root, parts, handlers);
  placeCaretAtOffset(root, caret);
  return parts;
}

/**
 * Moves the caret to the start/end of the current visual line (Cmd+Arrow on
 * macOS), hopping over any chip the native lineboundary movement stalls at.
 */
export function moveCaretToLineEdge(editor: HTMLElement, direction: "back" | "forward"): void {
  const selection = window.getSelection();
  if (!selection?.rangeCount || !editor.contains(selection.getRangeAt(0).startContainer)) return;
  if (typeof selection.modify !== "function") {
    // No Selection.modify (e.g. jsdom): fall back to the editor's edge.
    const range = document.createRange();
    range.selectNodeContents(editor);
    range.collapse(direction === "back");
    selection.removeAllRanges();
    selection.addRange(range);
    return;
  }
  for (let hops = 0; hops < 50; hops += 1) {
    selection.modify("move", direction === "back" ? "backward" : "forward", "lineboundary");
    const range = selection.getRangeAt(0);
    const chip = chipBesideCaret(direction, range);
    if (!chip) return;
    const beforeNode = range.startContainer;
    const beforeOffset = range.startOffset;
    placeCaretBesideChip(chip, direction === "back" ? "before" : "after");
    const moved = selection.getRangeAt(0);
    if (moved.startContainer === beforeNode && moved.startOffset === beforeOffset) return;
  }
}

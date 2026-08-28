/**
 * The composer's rich input as one object: the parts model, the caret, the
 * undo history and the trigger-character queries, bound to a contenteditable.
 *
 * Everything that has to agree with everything else lives here — a keystroke
 * that edits against the parts model rather than letting the browser have it,
 * the re-render that follows, the undo entry it makes and the picker query it
 * may have invalidated. The owner wires the DOM events to `handleKey` /
 * `handleInput` and reads `parts` and `queries`; it never assigns them. What
 * the editor does *not* decide (submitting, history recall, which picker is
 * open, attachment staging) comes in through `deps`.
 */
import {
  type AttachmentChipHandlers,
  type AttachmentPart,
  type ComposerPart,
  type DetectedQueries,
  EMPTY_PARTS,
  NO_QUERIES,
} from "$lib/composer/composerParts";
import {
  caretOffset,
  chipAcrossLineBreak,
  chipBesideCaret,
  deleteLineBreak,
  deleteToLineEdge,
  deleteToWordEdge,
  detectQueries,
  insertAttachmentChip,
  insertLineBreak,
  insertMentionChip,
  insertSkillChip,
  moveCaretToLineEdge,
  moveCaretToWordEdge,
  moveCaretVertically,
  normaliseEditorDom,
  placeCaretAtOffset,
  placeCaretBesideChip,
  readParts,
  removeMentionChip,
  renderPartsWith,
  updateAttachmentChip,
} from "$lib/composer/richInput";
import { type Snapshot, UndoStack } from "$lib/composer/undoStack";
import type { Mention } from "$lib/types";

export interface RichEditorDeps {
  /** Project root; mention queries are resolved against it. */
  cwd: () => string;
  /** Controls on attachment chips (retry, thumbnails). Staging is the owner's. */
  chipHandlers: AttachmentChipHandlers;
  /** A chip is about to leave the editor via Backspace/Delete, so the owner
   *  can unstage the file behind it. */
  onChipRemoved?: (chip: HTMLElement) => void;
  /** A picker is open with something to pick: Arrow/Enter/Tab belong to it. */
  pickerActive: () => boolean;
  /** Any picker is open: Escape belongs to it. */
  pickerOpen: () => boolean;
  /** Trigger detection is off (the review picker filters on the whole line). */
  suppressQueries?: () => boolean;
  /** Plain Enter. */
  onSubmit: () => void;
  /** ↑/↓ with no modifiers, asked before the caret moves. `true` consumes. */
  onHistory?: (direction: "older" | "newer") => boolean;
  /** The content changed — typed, edited, set or restored. */
  onEdit?: () => void;
  /** Clock for undo coalescing; injected by tests. */
  now?: () => number;
}

export class RichEditor {
  /** The parts model. Read by the owner, assigned only in here. */
  parts = $state<ComposerPart[]>(EMPTY_PARTS());
  /** Trigger queries before the caret, re-detected after every edit. */
  queries = $state.raw<DetectedQueries>(NO_QUERIES());

  private root: HTMLElement | null = null;
  private composing = false;
  private readonly undoStack: UndoStack;

  constructor(private readonly deps: RichEditorDeps) {
    this.undoStack = new UndoStack(deps.now);
    this.undoStack.reset({ parts: EMPTY_PARTS(), caret: 0 });
  }

  // --- lifecycle ---

  /** Bind to the contenteditable. Returns the matching detach. */
  attach(root: HTMLElement): () => void {
    this.root = root;
    return () => {
      if (this.root === root) this.root = null;
    };
  }

  get element(): HTMLElement | null {
    return this.root;
  }

  get canUndo(): boolean {
    return this.undoStack.canUndo;
  }

  get canRedo(): boolean {
    return this.undoStack.canRedo;
  }

  // --- DOM events (arrow fields so they can be passed as handlers) ---

  handleCompositionStart = (): void => {
    this.composing = true;
  };

  handleCompositionEnd = (): void => {
    this.composing = false;
    this.handleInput();
  };

  handleClick = (): void => {
    this.detect();
  };

  /** The browser wrote to the editor; read it back and record the edit. */
  handleInput = (): void => {
    if (this.composing) return;
    const root = this.root;
    if (!root) return;
    let next = readParts(root);
    // The browser answers Enter/paste with its own block lines and strips the
    // padding around chips; flatten it back before anything reads the caret.
    next = normaliseEditorDom(root, this.deps.chipHandlers) ?? next;
    this.adopt(next, true);
  };

  handleKey = (event: KeyboardEvent): void => {
    const root = this.root;
    const mod = event.metaKey || event.ctrlKey;
    if (mod && !event.altKey && (event.key === "z" || event.key === "Z" || event.key === "y")) {
      event.preventDefault();
      if (event.key === "y" || event.shiftKey) this.redo();
      else this.undo();
      return;
    }
    if (this.deps.pickerOpen() && event.key === "Escape") {
      event.preventDefault();
      return;
    }
    if (this.deps.pickerActive() && ["ArrowDown", "ArrowUp", "Enter", "Tab"].includes(event.key)) {
      // The pickers handle these through the window listener.
      event.preventDefault();
      return;
    }
    if ((event.key === "Backspace" || event.key === "Delete") && event.currentTarget === root && root) {
      const range = liveRange();
      const direction = event.key === "Backspace" ? "back" : "forward";
      if (event.metaKey) {
        // Delete-to-line-edge, against the parts model for the same reason as
        // `deleteLineBreak` below — and so it never degrades into "remove the
        // chip beside the caret", which is what the branches after this do.
        const afterLine = deleteToLineEdge(root, direction, this.deps.chipHandlers);
        if (afterLine) {
          event.preventDefault();
          this.adopt(afterLine);
        }
        return;
      }
      const chip = range ? chipBesideCaret(direction, range) : null;
      if (chip) {
        event.preventDefault();
        this.removeChip(chip);
        return;
      }
      if (event.altKey || event.ctrlKey) {
        // Option+Backspace/Delete word-deletion through a chip: WebKit's
        // native word motion can strand the caret at the very start of the
        // composer (and leave a stray line break behind) once whitespace next
        // to a contenteditable=false chip is involved. Ctrl is word-delete on
        // Windows/Linux and has no chip-safe native meaning on macOS, so it
        // takes the same path.
        const afterWord = deleteToWordEdge(root, direction, this.deps.chipHandlers);
        if (afterWord) {
          event.preventDefault();
          this.adopt(afterWord);
          return;
        }
      }
      // Line breaks are deleted against the parts model rather than by the
      // browser, which merges its own block lines with the composer's `<br>`s
      // and can take two breaks (or a whole line) for one keystroke.
      const afterBreak = deleteLineBreak(root, direction, this.deps.chipHandlers);
      if (afterBreak) {
        event.preventDefault();
        this.adopt(afterBreak);
        return;
      }
    }
    if ((event.key === "ArrowLeft" || event.key === "ArrowRight") && !event.shiftKey && !event.ctrlKey && root) {
      const direction = event.key === "ArrowLeft" ? "back" : "forward";
      // WebKit's native Cmd/Option+Arrow stalls at contenteditable=false chips.
      // Meta first, so Cmd+Option+Arrow keeps line semantics.
      if (event.metaKey) {
        event.preventDefault();
        moveCaretToLineEdge(root, direction);
        return;
      }
      if (event.altKey) {
        event.preventDefault();
        moveCaretToWordEdge(root, direction);
        return;
      }
      const range = liveRange();
      const chip = range ? chipBesideCaret(direction, range) : null;
      if (chip) {
        event.preventDefault();
        placeCaretBesideChip(chip, direction === "back" ? "before" : "after");
        return;
      }
      // A chip that starts/ends the line on the other side of a break (e.g. a
      // Shift+Enter typed just before it): WebKit won't cross the break on its
      // own, so land the caret beside the chip ourselves.
      const lineChip = range ? chipAcrossLineBreak(direction, range) : null;
      if (lineChip) {
        event.preventDefault();
        placeCaretBesideChip(lineChip, direction === "back" ? "after" : "before");
        return;
      }
    }
    if (
      (event.key === "ArrowDown" || event.key === "ArrowUp") &&
      !event.shiftKey &&
      !event.altKey &&
      !event.metaKey &&
      !event.ctrlKey &&
      root
    ) {
      // WebKit's native vertical caret motion refuses to move at all when the
      // line it's aiming for holds only a chip — from any column in the
      // current line, not just its very end — so it's driven by hand.
      if (this.deps.onHistory?.(event.key === "ArrowUp" ? "older" : "newer")) {
        event.preventDefault();
        return;
      }
      if (moveCaretVertically(root, event.key === "ArrowDown" ? "down" : "up")) {
        event.preventDefault();
      }
      return;
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      this.deps.onSubmit();
      return;
    }
    if (event.key === "Enter" && event.shiftKey && root) {
      // Breaks are inserted against the parts model for the same reason they
      // are deleted against it: beside a chip WebKit writes two `<br>`s and
      // strands the caret on the empty line between them.
      const afterBreak = insertLineBreak(root, this.deps.chipHandlers);
      if (afterBreak) {
        event.preventDefault();
        this.adopt(afterBreak);
      }
    }
  };

  // --- content ---

  /** Replace the content, put the caret at `caret` (default: the end) and
   *  record an undo entry. `focus: false` leaves focus and the caret alone,
   *  for content that arrives while the user is elsewhere. */
  setParts(parts: ComposerPart[], caret: number | null = null, { focus = true } = {}): void {
    this.render(parts, caret, focus);
    this.record(false);
    this.deps.onEdit?.();
  }

  /** Replace the content with plain text, caret at the end. */
  setText(text: string): void {
    this.setParts([{ type: "text", text }]);
  }

  /** Empty the editor and forget its history: the content left for good. */
  clear(): void {
    this.render(EMPTY_PARTS(), 0);
    this.resetUndo();
    this.deps.onEdit?.();
  }

  /** Replace the open `@query` with a chip. `false` when the query's range no
   *  longer points into the text (the picker is dismissed instead). */
  insertMention(mention: Mention): boolean {
    const range = this.queries.mentionRange;
    if (!this.usableRange(range)) {
      this.closeQueries();
      return false;
    }
    insertMentionChip(range, mention);
    this.afterChipInsert();
    return true;
  }

  /** Replace the open `$query` with a skill chip; see `insertMention`. */
  insertSkill(name: string, path: string, label: string): boolean {
    const range = this.queries.skillRange;
    if (!this.usableRange(range)) {
      this.closeQueries();
      return false;
    }
    insertSkillChip(range, name, path, label);
    this.afterChipInsert();
    return true;
  }

  /** Insert an attachment chip at the caret (or the end, when the caret is
   *  elsewhere). */
  insertAttachment(part: AttachmentPart): void {
    const root = this.root;
    if (!root) return;
    this.ensureCaretInside(root);
    insertAttachmentChip(part, this.deps.chipHandlers);
    this.adopt(readParts(root));
  }

  /** Rebuild the chip with `id` in place (staging → ready/failed). */
  updateAttachment(id: string, part: AttachmentPart): void {
    const root = this.root;
    if (!root) return;
    updateAttachmentChip(root, id, part, this.deps.chipHandlers);
    this.adopt(readParts(root));
  }

  /** Remove any chip (mention, skill or attachment), caret where it stood. */
  removeChip(chip: HTMLElement): void {
    this.deps.onChipRemoved?.(chip);
    removeMentionChip(chip);
    if (this.root) this.adopt(readParts(this.root));
    this.focus();
  }

  /** Forget the open trigger queries without touching the text. `detect`
   *  cannot do this: it bails when the caret is outside the editor, so losing
   *  focus would otherwise leave a picker stranded on screen. */
  closeQueries(): void {
    this.queries = NO_QUERIES();
  }

  // --- caret ---

  caretOffset(): number | null {
    return this.root ? caretOffset(this.root) : null;
  }

  /** Whether the caret sits on the first/last line of the content. */
  caretOnEdgeLine(edge: "first" | "last"): boolean {
    const offset = this.caretOffset();
    if (offset === null) return false;
    const text = this.parts.map((part) => (part.type === "text" ? part.text : " ")).join("");
    return edge === "first" ? !text.slice(0, offset).includes("\n") : !text.slice(offset).includes("\n");
  }

  focus(): void {
    this.root?.focus();
  }

  // --- history ---

  undo(): void {
    const snapshot = this.undoStack.undo();
    if (snapshot) this.restore(snapshot);
  }

  redo(): void {
    const snapshot = this.undoStack.redo();
    if (snapshot) this.restore(snapshot);
  }

  /** Make the current content the floor of the undo history. */
  resetUndo(): void {
    this.undoStack.reset(this.snapshot());
  }

  // --- internals ---

  /** Adopt parts produced by an edit: record it, re-detect the pickers and
   *  tell the owner.
   *
   *  Re-detection matters for the re-renders that bypass the browser (delete-
   *  to-edge, chip removal): they fire no `input` event, and a trigger the
   *  re-render removed would otherwise keep its picker open with a `Range`
   *  whose text node is gone. A live Range whose node is removed collapses
   *  onto the editor root at index 0, so the next pick would drop the chip at
   *  the very start of the composer, unpadded and unreachable by the
   *  Arrow/Backspace interception. */
  private adopt(parts: ComposerPart[], coalesce = false): void {
    this.parts = parts;
    this.record(coalesce);
    this.detect();
    this.deps.onEdit?.();
  }

  private render(parts: ComposerPart[], caret: number | null, focus = true): void {
    const root = this.root;
    if (!root) {
      this.parts = parts;
      return;
    }
    renderPartsWith(root, structuredClone(parts), this.deps.chipHandlers);
    this.parts = readParts(root);
    if (!focus) return;
    placeCaretAtOffset(root, caret ?? Number.MAX_SAFE_INTEGER);
    root.focus();
    this.detect();
  }

  private restore(snapshot: Snapshot): void {
    this.render(snapshot.parts, snapshot.caret);
    this.deps.onEdit?.();
  }

  private snapshot(): Snapshot {
    return { parts: $state.snapshot(this.parts) as ComposerPart[], caret: this.caretOffset() };
  }

  private record(coalesce: boolean): void {
    this.undoStack.record(this.snapshot(), coalesce);
  }

  private detect(): void {
    const root = this.root;
    if (!root) return;
    // While the review picker is up the line is its filter, not a trigger: `/`
    // or `@` typed into it must not open a second picker behind it.
    if (this.deps.suppressQueries?.()) return;
    const detected = detectQueries(root, this.deps.cwd());
    if (detected) this.queries = detected;
  }

  private afterChipInsert(): void {
    // The query is gone with the text it lived in; say so before re-reading,
    // in case the caret ends up somewhere `detect` cannot see.
    this.closeQueries();
    if (this.root) this.adopt(readParts(this.root));
    this.focus();
  }

  /** Whether a picker's range still points into the editor's current text. */
  private usableRange(range: Range | null): range is Range {
    return (
      !!range &&
      !!this.root &&
      range.startContainer.nodeType === Node.TEXT_NODE &&
      this.root.contains(range.startContainer)
    );
  }

  /** Focus the editor and drop the caret at its end when it's elsewhere. */
  private ensureCaretInside(root: HTMLElement): void {
    root.focus();
    const selection = window.getSelection();
    const inside = selection?.rangeCount && root.contains(selection.getRangeAt(0).startContainer);
    if (inside) return;
    const range = document.createRange();
    range.selectNodeContents(root);
    range.collapse(false);
    selection?.removeAllRanges();
    selection?.addRange(range);
  }
}

function liveRange(): Range | null {
  const selection = window.getSelection();
  return selection?.rangeCount ? selection.getRangeAt(0) : null;
}

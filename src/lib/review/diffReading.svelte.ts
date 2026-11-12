// Reading a page of diffs, one file after another: the review page and the
// Changes page. Which file is being read, which are folded, the selected lines
// and the composer on them, jumping between files and the keys that do it, and
// keeping the reader's place while the page's layout changes under them.
//
// - The current file follows the scroll: the first open file showing, measured
//   in a frame, where reading layout costs nothing the frame would not do
//   anyway and no Svelte flush is half done. It is measured again when the
//   files or the folds change, and when the page says its layout did.
// - A file the reader chose (a click on it, a jump, a selected line) stays
//   current until the page scrolls by more than a diff row or that file folds.
// - Scrolling down to the end of the page makes the last file current: short
//   files at the end never reach the top of the view.
// - Marking a file viewed folds it; unmarking opens it again.
// - Folding keeps the reader's place. WebKit has no scroll anchoring, so
//   otherwise folding a file whose header is above the view drops the reader
//   files further down. The current file's header comes back to the top of the
//   view when it folds from inside; otherwise it stays where it is on screen.
//
// The page supplies what differs between pages: its files, where the folds and
// the viewed marks are kept, and what a note on selected lines becomes.
// Construct it during component setup (it runs an effect), and attach the
// page's scroller with `{@attach reading.attach}`; each file's element there
// carries `data-file` with its path.

import { tick, untrack } from "svelte";
import type { Attachment } from "svelte/attachments";
import { dialogStack } from "$lib/app/dialogs.svelte";
import type { DiffSide, LineRange } from "$lib/review/reviewPage";
import type { PrFile } from "$lib/types";

/** The composer open on the selected lines. */
export type Composer = { range: LineRange; mode: "comment" | "suggestion" };

/** The folded files, by path. The page keeps them, so it decides how long they last. */
export type Folds = {
  has(path: string): boolean;
  add(path: string): unknown;
  delete(path: string): unknown;
};

/** The files marked viewed, by path, and where a change to them is saved. */
export type ViewedMarks = {
  has(path: string): boolean;
  set(files: readonly PrFile[], value: boolean): void;
};

/** A page-level command a key runs on a diff page. */
export type ReviewKeyCommand =
  | "nextPr"
  | "previousPr"
  | "nextFile"
  | "previousFile"
  | "nextUnresolved"
  | "toggleViewed"
  | "comment"
  | "submit"
  | "find"
  | "findNext"
  | "findPrevious"
  | "help";

/** What the page's own commands run. A command neither runs is left alone. */
export type PageKeys = Partial<Record<ReviewKeyCommand, () => void>>;

/** The review page's shortcuts, as the Shortcuts card lists them. */
export const REVIEW_SHORTCUTS: { keys: string[]; label: string }[] = [
  { keys: ["j", "k"], label: "Next / previous PR" },
  { keys: ["]", "["], label: "Next / previous file" },
  { keys: ["n"], label: "Next unresolved thread" },
  { keys: ["v"], label: "Viewed and collapse" },
  { keys: ["c"], label: "Comment on selected lines" },
  { keys: ["⌘↵"], label: "Submit review" },
  { keys: ["⌘F"], label: "Find on this tab" },
  { keys: ["?"], label: "Shortcuts" },
];

/** A chosen current file survives scrolling by less than a diff row. */
const HOLD_SLOP = 20;

/** A file with less than this still showing is finished: about two diff rows. */
const READ_MARGIN = 40;

export class DiffReading {
  readonly #files: () => readonly PrFile[];
  readonly #folds: Folds;
  readonly #viewed: ViewedMarks;
  readonly #addEntry: (range: LineRange, body: string, replacement: string | null) => void;
  readonly #showFiles: (() => void) | undefined;

  /** The file last chosen or measured, which may have left the list. */
  #chosen = $state<string | null>(null);
  #current = $derived.by(() => {
    const files = this.#files();
    const chosen = this.#chosen;
    return chosen !== null && files.some((file) => file.path === chosen) ? chosen : (files[0]?.path ?? null);
  });
  #selection = $state<LineRange | null>(null);
  #composer = $state<Composer | null>(null);

  #scroller: HTMLElement | null = null;
  #frame = 0;
  /** Where the page was when the reader chose the current file, or null. */
  #heldAt: number | null = null;
  #lastScrollTop = 0;

  constructor(options: {
    /** The files on the page, in page order. */
    files: () => readonly PrFile[];
    folds: Folds;
    viewed: ViewedMarks;
    /** Turn the composer's text into the page's note or comment on `range`. */
    addEntry: (range: LineRange, body: string, replacement: string | null) => void;
    /** Show the file list, when the page has other tabs. A jump calls it first. */
    showFiles?: () => void;
  }) {
    this.#files = options.files;
    this.#folds = options.folds;
    this.#viewed = options.viewed;
    this.#addEntry = options.addEntry;
    this.#showFiles = options.showFiles;

    // What is in view also changes when the list or a fold changes, from here
    // or not (viewed marks read on load fold their files).
    $effect(() => {
      for (const file of this.#files()) this.#folds.has(file.path);
      untrack(() => this.measure());
      return () => this.#cancelFrame();
    });
  }

  /** The file being read: measured from the scroll, or the one the reader
   *  chose. The first file until either says otherwise. */
  get currentPath(): string | null {
    return this.#current;
  }

  get selection(): LineRange | null {
    return this.#selection;
  }

  get composer(): Composer | null {
    return this.#composer;
  }

  isOpen(path: string): boolean {
    return !this.#folds.has(path);
  }

  /** The page's scroller, which holds the files' elements. */
  readonly attach: Attachment<HTMLElement> = (element) => {
    this.#scroller = element;
    const onScroll = () => this.measure();
    element.addEventListener("scroll", onScroll, { passive: true });
    this.measure();
    return () => {
      element.removeEventListener("scroll", onScroll);
      this.#cancelFrame();
      if (this.#scroller === element) this.#scroller = null;
    };
  };

  /** Measure the current file in the next frame: the page's layout changed in
   *  a way the files and folds don't show (another tab came back). */
  measure(): void {
    this.#frame ||= requestAnimationFrame(() => this.#measureNow());
  }

  /** The reader chose this file: it stays current until the page moves on.
   *  With `open`, unfold it too (without touching its viewed mark). */
  hold(path: string, options: { open?: boolean } = {}): void {
    this.#chosen = path;
    this.#heldAt = this.#scroller?.scrollTop ?? 0;
    if (options.open) this.#setOpen(path, true);
  }

  /** Fold or unfold a file the reader clicked, or with `alone` open it and
   *  fold every other. */
  toggle(path: string, alone = false): void {
    this.hold(path);
    void this.#keepingPlace(() => {
      if (alone) for (const file of this.#files()) this.#setOpen(file.path, file.path === path);
      else this.#setOpen(path, !this.isOpen(path));
    });
  }

  /** Fold or unfold every file on the page. */
  foldAll(folded: boolean): void {
    void this.#keepingPlace(() => {
      for (const file of this.#files()) this.#setOpen(file.path, !folded);
    });
  }

  /** Mark files viewed, which folds them, or not, which opens them; the page saves the marks. */
  setViewed(files: readonly PrFile[], value: boolean): void {
    void this.#keepingPlace(() => {
      this.#viewed.set(files, value);
      for (const file of files) this.#setOpen(file.path, !value);
    });
  }

  /** Open a file and scroll its top to the top of the view. It stays current
   *  after the scroll: a file near the end can't reach the top. */
  jumpTo(path: string): void {
    this.#showFiles?.();
    this.#chosen = path;
    this.#setOpen(path, true);
    // After the page has drawn the file.
    queueMicrotask(() => {
      this.#element(path)?.scrollIntoView?.({ block: "start" });
      this.hold(path);
    });
  }

  /** Select one line, or with `extend` grow the selection to it. A selection
   *  never spans files or sides; extending across starts afresh. */
  selectLine(path: string, side: DiffSide, line: number, extend: boolean): void {
    this.hold(path);
    this.#composer = null;
    const current = this.#selection;
    this.#selection =
      extend && current && current.path === path && current.side === side
        ? { ...current, start: Math.min(current.anchor, line), end: Math.max(current.anchor, line) }
        : { path, side, anchor: line, start: line, end: line };
  }

  /** Open the composer on the selected lines, or close it with `null`. */
  compose(mode: Composer["mode"] | null): void {
    if (mode === null) this.#composer = null;
    else if (this.#selection) this.#composer = { range: this.#selection, mode };
  }

  /** The composer's text becomes the page's entry; the selection is done. */
  add(body: string, replacement: string | null): void {
    const range = this.#composer?.range;
    if (!range) return;
    this.#addEntry(range, body, replacement);
    this.#composer = null;
    this.#selection = null;
  }

  /** The keys of a diff page. This runs ] and [ (next and previous file), v
   *  (viewed), c (comment on the selection) and Escape (drop the selection);
   *  `page` runs the rest. Keys typed into a field are the field's. */
  onKeydown(event: KeyboardEvent, page: PageKeys = {}): void {
    if (dialogStack.length > 0 || event.defaultPrevented) return;
    if (event.key === "Escape" && (this.#selection || this.#composer)) {
      this.#composer = null;
      this.#selection = null;
      return;
    }
    const command = reviewKey(event);
    const run = command && (this.#ownKey(command) ?? page[command]);
    if (!run) return;
    event.preventDefault();
    run();
  }

  #ownKey(command: ReviewKeyCommand): (() => void) | null {
    const files = this.#files();
    const index = files.findIndex((file) => file.path === this.currentPath);
    switch (command) {
      case "nextFile":
      case "previousFile":
        return () => {
          const next = files[Math.min(files.length - 1, Math.max(0, index + (command === "nextFile" ? 1 : -1)))];
          if (next) this.jumpTo(next.path);
        };
      case "toggleViewed":
        return () => {
          const current = files[index];
          if (current) this.setViewed([current], !this.#viewed.has(current.path));
        };
      case "comment":
        return () => this.compose("comment");
      default:
        return null;
    }
  }

  #setOpen(path: string, open: boolean): void {
    if (open) this.#folds.delete(path);
    else this.#folds.add(path);
  }

  #cancelFrame(): void {
    cancelAnimationFrame(this.#frame);
    this.#frame = 0;
  }

  #element(path: string): HTMLElement | null {
    return this.#scroller?.querySelector<HTMLElement>(`[data-file="${CSS.escape(path)}"]`) ?? null;
  }

  /** Where a file's top sits from the top of the view. Forces layout: call it
   *  from an event or after `tick`, not on scroll. */
  #offset(path: string): number | null {
    const element = this.#element(path);
    if (!this.#scroller || !element) return null;
    return element.getBoundingClientRect().top - this.#scroller.getBoundingClientRect().top;
  }

  async #keepingPlace(change: () => void): Promise<void> {
    const path = this.currentPath;
    const before = path ? this.#offset(path) : null;
    change();
    if (path === null || before === null) return;
    await tick();
    const after = this.#offset(path);
    if (this.#scroller && after !== null) {
      this.#scroller.scrollTop += after - (before < 0 && !this.isOpen(path) ? 0 : before);
    }
  }

  #measureNow(): void {
    this.#frame = 0;
    const scroller = this.#scroller;
    if (!scroller) return;
    const top = scroller.scrollTop;
    const movedDown = top > this.#lastScrollTop;
    this.#lastScrollTop = top;
    const current = this.currentPath;
    if (this.#heldAt !== null && Math.abs(top - this.#heldAt) <= HOLD_SLOP && !(current && !this.isOpen(current))) {
      return;
    }
    this.#heldAt = null;
    const elements = scroller.querySelectorAll<HTMLElement>("[data-file]");
    const view = scroller.getBoundingClientRect();
    const atEnd = movedDown && top + scroller.clientHeight >= scroller.scrollHeight - 1;
    const index = atEnd
      ? elements.length - 1
      : fileInView(
          elements.length,
          (at) => {
            const box = elements[at].getBoundingClientRect();
            return { top: box.top, bottom: box.bottom, open: this.isOpen(elements[at].dataset.file ?? "") };
          },
          view.top,
          view.bottom,
        );
    const path = elements[index]?.dataset.file;
    if (path) this.#chosen = path;
  }
}

/** A file's place on the page, in the same coordinates as the view. */
type FileBox = { top: number; bottom: number; open: boolean };

/** The file being read, as an index into `count` files in page order: the
 *  first open file showing, or the first file showing when all of those are
 *  collapsed. -1 when none is. `box` reads layout, so a binary search asks
 *  about few files. */
function fileInView(count: number, box: (index: number) => FileBox, viewTop: number, viewBottom: number) {
  let first = 0;
  let end = count;
  while (first < end) {
    const middle = (first + end) >> 1;
    if (box(middle).bottom > viewTop + READ_MARGIN) end = middle;
    else first = middle + 1;
  }
  for (let index = first; index < count; index += 1) {
    const at = box(index);
    if (at.top >= viewBottom) break;
    if (at.open) return index;
  }
  return first < count && box(first).top < viewBottom ? first : -1;
}

/** The command `event` presses, or `null`. Keys typed into a field are the
 *  field's, except ⌘↵ which submits from anywhere. */
function reviewKey(event: KeyboardEvent): ReviewKeyCommand | null {
  if ((event.metaKey || event.ctrlKey) && event.key === "Enter") return "submit";
  if ((event.metaKey || event.ctrlKey) && !event.altKey) {
    const key = event.key.toLowerCase();
    // With Shift, ⌘F is the sidebar's thread search.
    if (key === "f" && !event.shiftKey) return "find";
    if (key === "g") return event.shiftKey ? "findPrevious" : "findNext";
  }
  if (event.metaKey || event.ctrlKey || event.altKey) return null;
  const target = event.target as HTMLElement | null;
  if (target?.closest?.("input, textarea, select, [contenteditable='true']")) return null;
  const map: Record<string, ReviewKeyCommand> = {
    j: "nextPr",
    k: "previousPr",
    "]": "nextFile",
    "[": "previousFile",
    n: "nextUnresolved",
    v: "toggleViewed",
    c: "comment",
    "?": "help",
  };
  return map[event.key] ?? null;
}

import type { ComposerPart } from "$lib/composer/composerParts";

export interface Snapshot {
  parts: ComposerPart[];
  caret: number | null;
}

/** How long a run of edits keeps folding into one undo entry. */
export const COALESCE_MS = 500;
const CAP = 100;

/**
 * Undo/redo history for the composer's parts model. The browser's own undo
 * cannot be trusted here because the editor DOM is rewritten after every input;
 * this holds the parts snapshots instead. `now` is injected for tests.
 */
export class UndoStack {
  private past: Snapshot[] = [];
  private future: Snapshot[] = [];
  private lastRecordAt = Number.NEGATIVE_INFINITY;
  private lastCoalescable = false;

  constructor(private readonly now: () => number = () => Date.now()) {}

  /** The most recently recorded state, i.e. what the editor holds now. */
  get current(): Snapshot | null {
    return this.past.at(-1) ?? null;
  }

  get canUndo(): boolean {
    return this.past.length > 1;
  }

  get canRedo(): boolean {
    return this.future.length > 0;
  }

  /**
   * Record the editor's state after an edit. Consecutive `coalesce` records
   * within COALESCE_MS replace the top entry so a typed word undoes as a unit;
   * structural edits (breaks, chips, paste, history recall) always get their
   * own entry and end any run.
   */
  record(snapshot: Snapshot, coalesce = false): void {
    const at = this.now();
    const top = this.past.at(-1);
    if (top && sameContent(top, snapshot)) {
      top.caret = snapshot.caret;
      return;
    }
    this.future = [];
    if (coalesce && this.lastCoalescable && at - this.lastRecordAt < COALESCE_MS && this.past.length > 1) {
      this.past[this.past.length - 1] = snapshot;
    } else {
      this.past.push(snapshot);
      if (this.past.length > CAP) this.past.shift();
    }
    this.lastRecordAt = at;
    this.lastCoalescable = coalesce;
  }

  undo(): Snapshot | null {
    if (!this.canUndo) return null;
    this.future.push(this.past.pop() as Snapshot);
    this.lastCoalescable = false;
    return this.past.at(-1) ?? null;
  }

  redo(): Snapshot | null {
    const next = this.future.pop();
    if (!next) return null;
    this.past.push(next);
    this.lastCoalescable = false;
    return next;
  }

  reset(initial: Snapshot): void {
    this.past = [initial];
    this.future = [];
    this.lastCoalescable = false;
  }
}

function sameContent(a: Snapshot, b: Snapshot): boolean {
  return JSON.stringify(a.parts) === JSON.stringify(b.parts);
}

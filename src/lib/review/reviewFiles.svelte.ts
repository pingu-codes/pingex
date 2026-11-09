// Which of a PR's files the review page has marked viewed and which it has
// folded. Only the state and how the two move together: the page reads and
// saves the marks, and keeps the reader's place around each change.

import { SvelteSet } from "svelte/reactivity";
import type { PrFile } from "$lib/types";

export class ReviewFiles {
  readonly viewed = new SvelteSet<string>();
  readonly collapsed: SvelteSet<string>;
  /** `collapsed` is the reviewer's: the page came back with it, or the marks are read. */
  collapsedKnown = $state(false);
  readonly #hadCollapsed: boolean;

  /** `left` is what was folded when the page was left, or `null` the first time. */
  constructor(left: string[] | null) {
    this.collapsed = new SvelteSet(left ?? []);
    this.#hadCollapsed = left !== null;
    this.collapsedKnown = this.#hadCollapsed;
  }

  /** Take the marks just read for a head. */
  applyMarks(marked: string[]) {
    // Marks follow the file's content: one that changed is unread again.
    for (const path of this.viewed) if (!marked.includes(path)) this.collapsed.delete(path);
    this.viewed.clear();
    for (const path of marked) {
      this.viewed.add(path);
      // A page coming back keeps what the reviewer had folded and unfolded.
      if (!this.#hadCollapsed) this.collapsed.add(path);
    }
    this.collapsedKnown = true;
  }

  setOpen(path: string, open: boolean) {
    if (open) this.collapsed.delete(path);
    else this.collapsed.add(path);
  }

  setViewed(targets: PrFile[], value: boolean) {
    for (const file of targets) {
      if (value) this.viewed.add(file.path);
      else this.viewed.delete(file.path);
      // Marking viewed collapses; unmarking opens it again.
      this.setOpen(file.path, !value);
    }
  }

  toggle(path: string) {
    this.setOpen(path, this.collapsed.has(path));
  }

  /** Fold every file but this one. */
  openOnly(files: PrFile[], path: string) {
    for (const other of files) this.setOpen(other.path, other.path === path);
  }

  expandAll() {
    this.collapsed.clear();
  }

  collapseAll(files: PrFile[]) {
    for (const file of files) this.collapsed.add(file.path);
  }
}

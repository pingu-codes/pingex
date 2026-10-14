/**
 * What the composer's editor held before: the unsent draft persisted per
 * scope, and the ↑/↓ walk back through prior messages. Construct both during
 * component init: they register mount, destroy and effect hooks.
 */
import { onDestroy, onMount, untrack } from "svelte";
import {
  type ComposerPart,
  cloneParts,
  EMPTY_PARTS,
  hasSendableContent,
  normaliseParts,
} from "$lib/composer/composerParts";
import type { RichEditor } from "$lib/composer/richEditor.svelte";
import { commands } from "$lib/services/homeRouting";

/**
 * Draft persistence: unsent input is saved (debounced) under the draft key and
 * restored the next time a composer with that key mounts.
 */
export class ComposerDraft {
  private loaded = $state(false);
  /** The serialized form already on disk (null = no draft), so unchanged
   *  content never rewrites it. */
  private lastSaved: string | null = null;

  constructor(
    private readonly editor: RichEditor,
    private readonly key: () => string,
  ) {
    onMount(() => {
      let cancelled = false;
      (async () => {
        const draftKey = key();
        if (draftKey) {
          try {
            const stored = await commands.loadDraft(draftKey);
            const untouched = !editor.parts.some((part) => part.type !== "text" || part.text.length > 0);
            if (!cancelled && stored && untouched) {
              // The restored draft is the floor of the undo history, not an edit.
              editor.setParts(JSON.parse(stored) as ComposerPart[], null, { focus: false });
              editor.resetUndo();
              this.lastSaved = stored;
            }
          } catch {
            // An unreadable or corrupt draft should never block typing.
          }
        }
        if (!cancelled) this.loaded = true;
      })();
      return () => {
        cancelled = true;
      };
    });

    // A draft thread being adopted re-keys the composer without remounting.
    // The editor content follows the user into the new thread, so don't
    // reload — just forget what was written under the old key so the next
    // save isn't skipped.
    let scope = untrack(key);
    $effect(() => {
      if (key() === scope) return;
      scope = key();
      this.lastSaved = null;
    });

    $effect(() => {
      const serialized = this.serialize();
      if (!this.loaded || !key() || serialized === this.lastSaved) return;
      const timer = setTimeout(() => this.persist(serialized), 400);
      return () => clearTimeout(timer);
    });

    // The thread view is torn down on every navigation; flush what the
    // debounce hasn't written yet so nothing typed is lost.
    onDestroy(() => {
      if (this.loaded) this.persist(this.serialize());
    });
  }

  /** The content left with a send: drop the draft on disk now. */
  discard(): void {
    this.persist(null);
  }

  private serialize(): string | null {
    // Only ready attachments are worth persisting; a staging/failed one has no
    // usable staged path to restore.
    const persistable = this.editor.parts.filter((part) => part.type !== "attachment" || part.state === "ready");
    const parts = normaliseParts(cloneParts(persistable));
    return hasSendableContent(parts) ? JSON.stringify(parts) : null;
  }

  private persist(serialized: string | null): void {
    const draftKey = this.key();
    if (!draftKey || serialized === this.lastSaved) return;
    this.lastSaved = serialized;
    if (serialized === null) void commands.deleteDraft(draftKey);
    else void commands.saveDraft(draftKey, serialized);
  }
}

/**
 * ↑/↓ at the composer's first/last line walk back through prior messages, the
 * way a shell does; the unsent content is stashed on the way in and restored
 * once the walk runs off the newest end. Any edit ends the walk (`reset`).
 */
export class HistoryRecall {
  private index: number | null = null;
  private stash: ComposerPart[] | null = null;

  constructor(
    private readonly editor: RichEditor,
    private readonly entries: () => string[],
  ) {}

  /** The editor's content changed: the walk is over. */
  reset(): void {
    this.index = null;
  }

  /** Step the walk; `true` when the keystroke was consumed. */
  recall(direction: "older" | "newer"): boolean {
    const editor = this.editor;
    const entries = this.entries();
    if (!editor.parts.every((part) => part.type === "text")) return false;
    if (direction === "older") {
      if (!editor.caretOnEdgeLine("first")) return false;
      const next = (this.index ?? entries.length) - 1;
      if (next < 0) return entries.length !== 0;
      if (this.index === null) this.stash = $state.snapshot(editor.parts) as ComposerPart[];
      editor.setText(entries[next] ?? "");
      this.index = next;
      return true;
    }
    if (this.index === null || !editor.caretOnEdgeLine("last")) return false;
    const next = this.index + 1;
    if (next >= entries.length) {
      const stash = this.stash ?? EMPTY_PARTS();
      this.stash = null;
      editor.setParts(stash);
      this.index = null;
      return true;
    }
    editor.setText(entries[next] ?? "");
    this.index = next;
    return true;
  }
}

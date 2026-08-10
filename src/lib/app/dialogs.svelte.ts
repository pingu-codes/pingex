/**
 * Imperative dialogs: `await openDialog(SomeDialog, props)` instead of every
 * parent owning `open`/`busy`/`error` state and rendering the component inline.
 * A single <DialogHost /> renders the stack, so a dialog exists only while it
 * is open and any component can raise one without threading props through the
 * tree.
 */
import type { Component } from "svelte";

/** Every dialog receives this; calling it dismisses the dialog and resolves
 *  the promise `openDialog` returned. No argument means "dismissed". */
export type DialogClose<Result> = (result?: Result) => void;

type Entry = {
  id: number;
  // The stack is heterogeneous, so entries are erased to `any` here; the
  // generics on `openDialog` keep the call sites type-safe.
  // biome-ignore lint/suspicious/noExplicitAny: heterogeneous dialog stack
  component: Component<any>;
  props: Record<string, unknown>;
  resolve: (result: unknown) => void;
};

export const dialogStack = $state<Entry[]>([]);

let nextId = 0;

/**
 * Mount `component` on top of the stack and resolve once it closes: with the
 * value it passed to `close()`, or `null` when dismissed (Esc, backdrop, the
 * close button, Cancel).
 */
export function openDialog<Result, Props extends Record<string, unknown>>(
  component: Component<Props & { close: DialogClose<Result> }>,
  props: Props,
): Promise<Result | null> {
  return new Promise<Result | null>((resolve) => {
    dialogStack.push({
      id: nextId++,
      component,
      props,
      resolve: resolve as (result: unknown) => void,
    });
  });
}

/** Called by DialogHost when a dialog closes itself. */
export function closeDialog(id: number, result: unknown): void {
  const index = dialogStack.findIndex((entry) => entry.id === id);
  if (index === -1) return;
  const [entry] = dialogStack.splice(index, 1);
  entry.resolve(result ?? null);
}

/** Dismiss everything — used when the app switches Codex home under a dialog. */
export function closeAllDialogs(): void {
  for (const entry of dialogStack.splice(0)) entry.resolve(null);
}

/**
 * Busy/error state for a dialog that performs the work itself and stays open
 * when it fails (rather than resolving and letting the caller report the
 * error out of context). `run` returns whether the action succeeded.
 */
export function submitState() {
  let busy = $state(false);
  let error = $state<string | null>(null);
  return {
    get busy() {
      return busy;
    },
    get error() {
      return error;
    },
    set error(value: string | null) {
      error = value;
    },
    async run(action: () => Promise<void>): Promise<boolean> {
      if (busy) return false;
      busy = true;
      error = null;
      try {
        await action();
        return true;
      } catch (cause) {
        error = cause instanceof Error ? cause.message : String(cause);
        return false;
      } finally {
        busy = false;
      }
    },
  };
}

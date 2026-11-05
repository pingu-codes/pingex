import { createToaster } from "@skeletonlabs/skeleton-svelte";

/** Singleton toaster; rendered by <ToastHost /> in App.svelte. */
export const toaster = createToaster({
  placement: "bottom-end",
  duration: 6000,
  max: 4,
});

export function toastError(message: string) {
  toaster.error({ description: message, closable: true });
}

/** A toast offering Undo, for a quick action that put something on disk.
 *  Returns the toast's id. */
export function toastUndo(title: string, undo: () => void | Promise<void>, description?: string): string {
  return toaster.create({
    type: "info",
    title,
    description,
    duration: 10000,
    closable: true,
    action: { label: "Undo", onClick: () => void undo() },
  });
}

/** A toast for background work that stays until `toaster.dismiss(id)`. */
export function toastProgress(title: string, description?: string): string {
  return toaster.create({ type: "info", title, description, duration: Number.POSITIVE_INFINITY, closable: true });
}

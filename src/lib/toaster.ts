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

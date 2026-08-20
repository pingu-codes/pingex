<script lang="ts">
import { Toast } from "@skeletonlabs/skeleton-svelte";
import { toaster } from "./toaster";
</script>

<Toast.Group {toaster} class="z-50 gap-2 p-4">
  {#snippet children(toast)}
    <Toast
      {toast}
      class="card flex max-w-sm items-start gap-3 px-3 py-2 text-xs shadow-lg
        {toast.type === 'error' ? 'preset-tonal-error' : toast.type === 'success' ? 'preset-tonal-success' : toast.type === 'warning' ? 'preset-tonal-warning' : 'preset-tonal'}"
    >
      <Toast.Message class="flex-1">
        {#if toast.title}<Toast.Title class="font-semibold">{toast.title}</Toast.Title>{/if}
        {#if toast.description}<Toast.Description>{toast.description}</Toast.Description>{/if}
      </Toast.Message>
      <Toast.CloseTrigger class="opacity-60 hover:opacity-100" aria-label="Dismiss">✕</Toast.CloseTrigger>
    </Toast>
  {/snippet}
</Toast.Group>

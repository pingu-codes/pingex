<script lang="ts">
/**
 * The chrome every dialog shares: Skeleton's Dialog primitive plus the
 * backdrop / centred card / title row / close button. Dialogs are mounted by
 * DialogHost only while they are open, so the shell is always open and reports
 * every dismissal (Esc, backdrop, close button) through `onClose`.
 */
import { X } from "@lucide/svelte";
import { Dialog, Portal } from "@skeletonlabs/skeleton-svelte";
import type { Snippet } from "svelte";

let {
  title,
  subtitle = null,
  titleClass = "",
  width = 420,
  onClose,
  icon,
  children,
  footer,
}: {
  title: string;
  subtitle?: string | null;
  /** Extra classes for the title row (e.g. a destructive colour). */
  titleClass?: string;
  /** Max card width in pixels; the card shrinks on narrow windows. */
  width?: number;
  onClose: () => void;
  icon?: Snippet;
  children: Snippet;
  /** Action buttons; laid out right-aligned (use `mr-auto` to push left). */
  footer?: Snippet;
} = $props();
</script>

<Dialog open onOpenChange={(details) => !details.open && onClose()}>
  <Portal>
    <Dialog.Backdrop class="fixed inset-0 z-40 bg-surface-950/40 backdrop-blur-[2px]" />
    <Dialog.Positioner class="fixed inset-0 z-50 grid place-items-center p-4">
      <Dialog.Content
        class="card max-h-[calc(100vh-2rem)] w-full overflow-y-auto border border-surface-200-800 bg-surface-50-950 p-5 shadow-2xl outline-none"
        style="max-width: {width}px"
      >
        <div class="flex items-start justify-between gap-4">
          <div class="min-w-0">
            <Dialog.Title class="flex items-center gap-2 text-base font-semibold {titleClass}">
              {@render icon?.()}
              {title}
            </Dialog.Title>
            {#if subtitle}
              <p class="mt-1 text-xs leading-5 text-surface-500">{subtitle}</p>
            {/if}
          </div>
          <Dialog.CloseTrigger class="btn-icon btn-icon-sm shrink-0 hover:preset-tonal text-surface-500" aria-label="Close">
            <X size={16} />
          </Dialog.CloseTrigger>
        </div>

        {@render children()}

        {#if footer}
          <div class="mt-5 flex items-center justify-end gap-2">
            {@render footer()}
          </div>
        {/if}
      </Dialog.Content>
    </Dialog.Positioner>
  </Portal>
</Dialog>

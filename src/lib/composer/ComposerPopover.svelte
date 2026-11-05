<script lang="ts">
  import { Popover } from "@skeletonlabs/skeleton-svelte";
  import type { Snippet } from "svelte";

  let {
    open,
    onToggle,
    triggerLabel,
    triggerClass = "",
    label,
    contentClass = "",
    trigger,
    children,
  }: {
    /** Controlled, so slash commands can open the popover from code. */
    open: boolean;
    /** Asked to flip `open`: trigger click, outside click or Escape. */
    onToggle: () => void;
    /** Accessible name of the trigger button. */
    triggerLabel: string;
    triggerClass?: string;
    /** Accessible name of the popover dialog. */
    label: string;
    /** Extra classes for the card, typically its width. */
    contentClass?: string;
    trigger: Snippet;
    children: Snippet;
  } = $props();
</script>

<!--
  Not portalled: the popover may sit inside the toolbar's More panel, whose
  outside-click and Escape handling treat anything outside it as outside. The
  fixed strategy lets the card escape that panel's scroll clipping.
-->
<div class="relative">
  <Popover
    {open}
    onOpenChange={(details) => {
      if (details.open !== open) onToggle();
    }}
    portalled={false}
    positioning={{ placement: "top-start", strategy: "fixed", gutter: 8, overflowPadding: 8 }}
  >
    <Popover.Trigger aria-label={triggerLabel} class="pill {triggerClass}"> {@render trigger()} </Popover.Trigger>
    <Popover.Positioner>
      <Popover.Content
        aria-label={label}
        class="popover-card z-[70] max-h-[calc(100vh-16px)] max-w-[calc(100vw-16px)] select-none overflow-y-auto p-2 {contentClass}"
      >
        {#if open}
          {@render children()}
        {/if}
      </Popover.Content>
    </Popover.Positioner>
  </Popover>
</div>

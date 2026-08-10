<script lang="ts">
import { Portal, Tooltip } from "@skeletonlabs/skeleton-svelte";
import type { Snippet } from "svelte";
import type { HTMLButtonAttributes } from "svelte/elements";

type Props = HTMLButtonAttributes & {
  label: string;
  children: Snippet;
  openDelay?: number;
};

let { label, children, openDelay = 350, ...buttonProps }: Props = $props();
</script>

<Tooltip {openDelay} positioning={{ placement: "top" }}>
  <Tooltip.Trigger {...buttonProps} aria-label={buttonProps["aria-label"] ?? label}>
    {@render children()}
  </Tooltip.Trigger>
  <Portal>
    <Tooltip.Positioner>
      <Tooltip.Content
        class="pointer-events-none z-[100] max-w-[16rem] rounded-lg border border-white/10 bg-surface-900/80 px-2.5 py-1.5 text-center text-[11px] leading-4 font-medium text-white shadow-lg backdrop-blur-md dark:border-surface-900/10 dark:bg-white/80 dark:text-surface-900"
      >{label}</Tooltip.Content>
    </Tooltip.Positioner>
  </Portal>
</Tooltip>

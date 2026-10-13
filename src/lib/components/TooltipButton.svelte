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
        class="tooltip-bubble max-w-[16rem]"
      >{label}</Tooltip.Content>
    </Tooltip.Positioner>
  </Portal>
</Tooltip>

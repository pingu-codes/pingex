<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLAttributes, HTMLButtonAttributes } from "svelte/elements";
  import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";

  type Props = HTMLButtonAttributes & {
    label: string;
    children: Snippet;
    openDelay?: number;
  };

  let { label, children, openDelay = 350, ...buttonProps }: Props = $props();
</script>

<TooltipAnchor
  tag="button"
  {label}
  {openDelay}
  {...buttonProps as HTMLAttributes<HTMLElement>}
  aria-label={buttonProps["aria-label"] ?? label}
>
  {@render children()}
</TooltipAnchor>

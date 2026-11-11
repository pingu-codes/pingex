<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLAttributes, HTMLButtonAttributes } from "svelte/elements";
  import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";

  type Props = HTMLButtonAttributes & {
    label: string;
    children: Snippet;
    openDelay?: number;
    /** Keep the label's line breaks, for a tooltip that quotes text. */
    multiline?: boolean;
  };

  let { label, children, openDelay = 350, multiline = false, ...buttonProps }: Props = $props();
</script>

<TooltipAnchor
  tag="button"
  {label}
  {openDelay}
  {multiline}
  {...buttonProps as HTMLAttributes<HTMLElement>}
  aria-label={buttonProps["aria-label"] ?? label}
>
  {@render children()}
</TooltipAnchor>

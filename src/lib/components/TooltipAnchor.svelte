<script lang="ts">
import { Portal, Tooltip } from "@skeletonlabs/skeleton-svelte";
import type { Snippet } from "svelte";
import type { HTMLAttributes, SvelteHTMLElements } from "svelte/elements";

type Props = HTMLAttributes<HTMLSpanElement> & {
  label: string;
  children: Snippet;
  openDelay?: number;
  multiline?: boolean;
};

let { label, children, openDelay = 350, multiline = false, ...anchorProps }: Props = $props();
</script>

{#snippet trigger(attributes: SvelteHTMLElements["button"])}
  {@const spanAttributes = attributes as SvelteHTMLElements["span"]}
  <span {...spanAttributes} {...anchorProps}>{@render children()}</span>
{/snippet}

<Tooltip {openDelay} positioning={{ placement: "top" }}>
  <Tooltip.Trigger element={trigger} />
  <Portal>
    <Tooltip.Positioner>
      <Tooltip.Content
        class="tooltip-bubble {multiline ? 'max-w-[18rem] whitespace-pre-line break-words' : 'max-w-[16rem]'}"
      >{label}</Tooltip.Content>
    </Tooltip.Positioner>
  </Portal>
</Tooltip>

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
        class="pointer-events-none z-[100] {multiline ? 'max-w-[18rem] whitespace-pre-line break-words' : 'max-w-[16rem]'} rounded-lg border border-white/10 bg-surface-900/80 px-2.5 py-1.5 text-center text-[11px] leading-4 font-medium text-white shadow-lg backdrop-blur-md dark:border-surface-900/10 dark:bg-white/80 dark:text-surface-900"
      >{label}</Tooltip.Content>
    </Tooltip.Positioner>
  </Portal>
</Tooltip>

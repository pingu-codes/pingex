<script lang="ts">
  import { Portal, Tooltip } from "@skeletonlabs/skeleton-svelte";
  import type { Snippet } from "svelte";
  import type { HTMLAttributes, SvelteHTMLElements } from "svelte/elements";

  type Placement = "top" | "top-start" | "top-end" | "bottom" | "bottom-start" | "bottom-end";

  type Props = HTMLAttributes<HTMLElement> & {
    label: string;
    children: Snippet;
    openDelay?: number;
    multiline?: boolean;
    /** The anchor element; TooltipButton renders through here with "button". */
    tag?: "span" | "button";
    placement?: Placement;
    /** Rich tooltip body in place of `label`, drawn in `contentClass` instead of the default bubble. */
    content?: Snippet;
    contentClass?: string;
  };

  let {
    label,
    children,
    openDelay = 350,
    multiline = false,
    tag = "span",
    placement = "top",
    content,
    contentClass,
    ...anchorProps
  }: Props = $props();

  const bubbleClass = $derived(
    `tooltip-bubble ${multiline ? "max-w-[18rem] whitespace-pre-line break-words" : "max-w-[16rem]"}`,
  );
</script>

{#snippet trigger(attributes: SvelteHTMLElements["button"])}
  {#if tag === "button"}
    <button {...attributes}>{@render children()}</button>
  {:else}
    {@const spanAttributes = attributes as SvelteHTMLElements["span"]}
    <span {...spanAttributes}>{@render children()}</span>
  {/if}
{/snippet}

<Tooltip {openDelay} positioning={{ placement }}>
  <Tooltip.Trigger {...anchorProps as SvelteHTMLElements["button"]} element={trigger} />
  <Portal>
    <Tooltip.Positioner>
      <Tooltip.Content class={contentClass ? `pointer-events-none z-[100] ${contentClass}` : bubbleClass}
        >{#if content}
          {@render content()}
        {:else}
          {label}
        {/if}</Tooltip.Content
      >
    </Tooltip.Positioner>
  </Portal>
</Tooltip>

<!--
  The popup shared by the composer's three triggers: `/` (commands), `@` (file
  mentions), and `$` (skills).

  It owns everything the three had in common — the card chrome, the active-index
  clamp, and Arrow/Enter/Tab/Escape handling — so a picker only has to supply
  its items and how a row looks.

  Keys are read from a window listener because the composer's contenteditable
  keeps focus while the popup is open, so the popup itself never receives them.
  `scope` bounds that: without it, arrow keys pressed anywhere in the app would
  drive whichever list happened to be open.
-->
<script lang="ts" generics="T">
import type { Snippet } from "svelte";

let {
  items,
  label,
  emptyMessage,
  error = null,
  scope = null,
  onPick,
  onClose,
  onCount,
  key,
  row,
}: {
  items: T[];
  /** Accessible name for the listbox. */
  label: string;
  /** Shown when `items` is empty and there is no `error`. */
  emptyMessage: string;
  /** Shown instead of the list when the source failed. */
  error?: string | null;
  /** Only keys originating inside this element drive the list. */
  scope?: HTMLElement | null;
  onPick: (item: T) => void;
  onClose: () => void;
  /**
   * How many results are showing. The composer needs this to decide whether to
   * swallow Enter: an open-but-empty picker must not eat the keystroke, or
   * `/notacommand` and `@nothingmatches` become unsendable.
   */
  onCount?: (count: number) => void;
  /** Stable identity for each row, so `{#each}` keys correctly. */
  key: (item: T) => string;
  row: Snippet<[T, boolean]>;
} = $props();

let active = $state(0);

// Results are refetched as the query changes; keep the cursor in range.
$effect(() => {
  if (active >= items.length) active = 0;
});

$effect(() => {
  onCount?.(items.length);
});

function onWindowKeydown(event: KeyboardEvent) {
  if (scope && event.target instanceof Node && !scope.contains(event.target)) return;
  if (event.key === "ArrowDown") {
    active = Math.min(active + 1, items.length - 1);
  } else if (event.key === "ArrowUp") {
    active = Math.max(active - 1, 0);
  } else if (event.key === "Enter" || event.key === "Tab") {
    if (items[active]) onPick(items[active]);
  } else if (event.key === "Escape") {
    onClose();
  }
}
</script>

<svelte:window onkeydown={onWindowKeydown} />

<div
  class="card absolute bottom-full left-0 z-50 mb-2 max-h-[min(320px,50vh)] w-[min(420px,100%)] select-none overflow-y-auto border border-surface-200-800 bg-surface-50-950 p-1 shadow-xl"
  role="listbox"
  aria-label={label}
>
  {#if error}
    <p class="px-2 py-2 text-xs text-error-500">{error}</p>
  {:else if items.length === 0}
    <p class="px-2 py-2 text-xs text-surface-500">{emptyMessage}</p>
  {:else}
    {#each items as item, index (key(item))}
      <button
        role="option"
        aria-selected={index === active}
        onmouseenter={() => (active = index)}
        onclick={() => onPick(item)}
        class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs {index === active
          ? 'preset-tonal'
          : ''}"
      >
        {@render row(item, index === active)}
      </button>
    {/each}
  {/if}
</div>

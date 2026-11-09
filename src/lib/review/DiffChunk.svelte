<!--
  A run of diff rows that mounts only near the Diffs pane's viewport, so a
  long PR keeps a few screens of rows in the DOM, not all of them. Away from
  it, the rows are an empty box of the same size: each `.ln` row is 20px and
  never wraps, so the box is exact, and it is as wide as the longest line, so
  the file's horizontal scroll holds. `eager` keeps it mounted anywhere, for
  rows with threads, cards or a composer, whose heights the box can't know.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { nearViewport } from "$lib/review/nearViewport";

  let {
    rows,
    columns,
    eager,
    children,
  }: {
    /** How many `.ln` rows it holds. */
    rows: number;
    /** Its longest line, in characters. */
    columns: number;
    eager: boolean;
    children: Snippet;
  } = $props();

  let near = $state(false);
</script>

<div data-rows={rows} {@attach nearViewport((value) => (near = value))}>
  {#if eager || near}
    {@render children()}
  {:else}
    <div class="font-mono text-xs" style:height="{rows * 20}px" style:width="calc(104px + {columns}ch)"></div>
  {/if}
</div>

<script lang="ts">
import type { Snippet } from "svelte";

// One titled card in the Settings content pane.
let {
  title,
  description,
  error = null,
  children,
}: {
  title: string;
  /** One line under the title; a snippet when it needs markup. */
  description?: string | Snippet;
  /** Shown under the header when the card's last load or save failed. */
  error?: string | null;
  children: Snippet;
} = $props();
</script>

<section class="card border border-surface-200-800 bg-surface-50-950 p-5">
  <h2 class="text-sm font-semibold">{title}</h2>
  {#if description}
    <p class="mt-1 text-xs text-surface-600-400">
      {#if typeof description === "string"}{description}{:else}{@render description()}{/if}
    </p>
  {/if}
  {#if error}
    <div class="error-note mt-3">{error}</div>
  {/if}
  {@render children()}
</section>

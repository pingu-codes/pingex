<script lang="ts">
// An image the agent looked at or generated, shown inline under a one-line
// caption. Falls back to the caption alone when the file cannot be loaded
// (browser preview, deleted file, not an image).
import { Image } from "@lucide/svelte";
import { revealInFinder, threadImageSrc } from "$lib/services/api";

let { path, label }: { path: string; label: string } = $props();

let src = $state<string | null>(null);
let failed = $state(false);
let expanded = $state(false);

$effect(() => {
  const current = path;
  src = null;
  failed = false;
  void threadImageSrc(current).then((url) => {
    if (current === path) src = url;
  });
});
</script>

<div class="flex min-w-0 flex-col gap-1.5">
  <div class="flex items-center gap-2 text-xs text-surface-500">
    <Image size={12} class="shrink-0" />
    <button
      type="button"
      onclick={() => revealInFinder(path).catch(() => {})}
      title={path}
      class="min-w-0 truncate text-left font-mono text-[11px] hover:text-surface-700-300"
    >
      {label}
    </button>
  </div>
  {#if src && !failed}
    <button
      type="button"
      onclick={() => (expanded = !expanded)}
      aria-label={expanded ? "Shrink image" : "Expand image"}
      class="w-fit max-w-full cursor-zoom-in overflow-hidden rounded-lg border border-surface-200-800"
      class:cursor-zoom-out={expanded}
    >
      <img
        {src}
        alt={path.split("/").pop() || path}
        loading="lazy"
        class={`block max-w-full object-contain ${expanded ? "" : "max-h-72"}`}
        onerror={() => (failed = true)}
      />
    </button>
  {/if}
</div>

<script lang="ts">
import { Ellipsis } from "@lucide/svelte";
import { type Snippet, tick } from "svelte";

type Item = { id: string; priority: number; content: Snippet; placement?: "beforeModel" | "end" };
const panelId = $props.id();
let {
  attachment,
  model,
  context,
  items,
  onLayoutChange,
  activeItem,
}: {
  attachment: Snippet;
  model: Snippet;
  context: Snippet;
  items: Item[];
  onLayoutChange: () => void;
  activeItem: string | null;
} = $props();
let root: HTMLDivElement;
let trigger = $state<HTMLButtonElement>();
let panel: HTMLDivElement;
let widths = $state<Record<string, number>>({});
let available = $state(0);
let hidden = $state<string[]>([]);
let open = $state(false);
let position = $state({ left: 0, bottom: 0 });

function measure(node: HTMLElement, id: string) {
  const update = () => {
    const width = node.getBoundingClientRect().width;
    if (id === "root") available = width;
    else if (widths[id] !== width) widths = { ...widths, [id]: width };
  };
  update();
  const observer = new ResizeObserver(update);
  observer.observe(node);
  return { destroy: () => observer.disconnect() };
}

$effect(() => {
  if (available <= 0) return;
  const gap = 6;
  const fixed = (widths.attachment ?? 28) + Math.min(220, Math.max(60, available - 150)) + (widths.context ?? 28);
  let used = fixed + items.reduce((sum, item) => sum + (widths[item.id] ?? 90), 0) + gap * (items.length + 2);
  const next: string[] = [];
  if (used > available) {
    used += 58 + gap;
    for (const item of [...items].sort((a, b) => a.priority - b.priority)) {
      if (used <= available) break;
      next.push(item.id);
      used -= (widths[item.id] ?? 90) + gap;
    }
  }
  if (next.join() !== hidden.join()) {
    const active = document.activeElement;
    const activeId =
      active instanceof HTMLElement
        ? active.closest<HTMLElement>("[data-toolbar-item]")?.dataset.toolbarItem
        : undefined;
    const movedFocus = active === trigger || Boolean(activeId && hidden.includes(activeId) !== next.includes(activeId));
    hidden = next;
    open = false;
    onLayoutChange();
    if (movedFocus)
      void tick().then(() => (next.length ? trigger : root.querySelector<HTMLButtonElement>("button"))?.focus());
  }
});

$effect(() => {
  if (activeItem && hidden.includes(activeItem)) {
    place();
    open = true;
  }
});

function place() {
  if (!trigger) return;
  const rect = trigger.getBoundingClientRect();
  position = {
    left: Math.max(8, Math.min(rect.right - 300, window.innerWidth - 308)),
    bottom: Math.max(8, window.innerHeight - rect.top + 8),
  };
}
function close(restore = false) {
  open = false;
  onLayoutChange();
  if (restore) trigger?.focus();
}
async function toggle() {
  if (open) {
    close(true);
    return;
  }
  place();
  open = true;
  await tick();
  panel?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
}
</script>

<svelte:window onresize={place} onclick={(event) => {
  if (open && !root.contains(event.target as Node)) close();
}} onkeydown={(event) => {
  if (open && event.key === 'Escape' && !event.defaultPrevented) {
    event.preventDefault();
    event.stopImmediatePropagation();
    close(true);
  }
}} />

<div bind:this={root} use:measure={'root'} class="toolbar" data-testid="composer-toolbar">
  <div class="control" use:measure={'attachment'}>{@render attachment()}</div>
  {#each items.filter(item => !hidden.includes(item.id) && item.placement === 'beforeModel') as item (item.id)}
    <div class="control" data-toolbar-item={item.id} use:measure={item.id}>{@render item.content()}</div>
  {/each}
  <div class="model" use:measure={'model'}>{@render model()}</div>
  {#each items.filter(item => !hidden.includes(item.id) && !item.placement) as item (item.id)}
    <div class="control" data-toolbar-item={item.id} use:measure={item.id}>{@render item.content()}</div>
  {/each}
  {#each items.filter(item => !hidden.includes(item.id) && item.placement === 'end') as item (item.id)}
    <div class="control home" data-toolbar-item={item.id} use:measure={item.id}>{@render item.content()}</div>
  {/each}
  {#if hidden.length}
    <button bind:this={trigger} class="more" aria-label="More composer options" aria-expanded={open} aria-haspopup="dialog" aria-controls={panelId} onclick={(event) => { event.stopPropagation(); void toggle(); }}><Ellipsis size={14} /> More</button>
  {/if}
  <div class="control context" use:measure={'context'}>{@render context()}</div>
  <!-- Hidden controls retain measurable intrinsic widths without duplicate controls. -->
  <div id={panelId} bind:this={panel} class="overflow card border border-surface-200-800 bg-surface-50-950 shadow-xl" class:closed={!open}
    style:left="{position.left}px" style:bottom="{position.bottom}px"
    style:max-height="calc(100vh - {position.bottom + 8}px)"
    role="dialog" aria-label="More composer options" aria-hidden={!open} inert={!open}
    onclick={(event) => event.stopPropagation()} onkeydown={(event) => {
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(true); }
    }} tabindex="-1">
    {#each items.filter(item => hidden.includes(item.id)) as item (item.id)}
      <div class="control" data-toolbar-item={item.id} use:measure={item.id}>{@render item.content()}</div>
    {/each}
  </div>
</div>

<style>
.toolbar { display: flex; align-items: center; gap: 6px; margin-top: 6px; min-width: 0; white-space: nowrap; }
.control { flex: none; width: max-content; }
.model { flex: none; min-width: 0; width: clamp(60px, calc(100% - 150px), 220px); }
.model :global(> div), .model :global(button) { min-width: 0; max-width: 100%; }
.model :global(button > svg) { flex-shrink: 0; }
.toolbar :global([role="dialog"]) { white-space: normal; }
.home, .context { margin-left: auto; }
.more { display: inline-flex; flex: none; align-items: center; gap: 4px; padding: 4px 6px; border-radius: 999px; font-size: 11px; }
.more:hover { background: var(--color-surface-200-800); }
.overflow { position: fixed; z-index: 60; width: min(300px, calc(100vw - 16px)); max-height: calc(100vh - 80px); padding: 8px; overflow-y: auto; display: flex; flex-direction: column; align-items: flex-start; gap: 6px; white-space: nowrap; }
.closed { visibility: hidden; pointer-events: none; }
</style>

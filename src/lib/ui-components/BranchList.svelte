<script lang="ts">
  import { GitBranch } from "@lucide/svelte";
  import { filterBranches } from "$lib/composer/reviewTargets";
  import type { GitBranch as Branch } from "$lib/types";

  // A filter box over a branch listbox, with its loading, error and empty
  // states. `combobox` makes the filter drive the list from the keyboard
  // (arrows move the highlight, Enter picks); otherwise rows are only clicked.
  let {
    id,
    branches,
    remote = [],
    query = $bindable(""),
    onQueryInput,
    label,
    showLabel = false,
    placeholder = "Filter branches",
    combobox = false,
    loaded = true,
    error = null,
    currentLabel = "current",
    selected = (branch: Branch) => branch.isCurrent,
    disabled = (branch: Branch) => branch.isCurrent,
    onPick,
    listClass = "max-h-64",
    class: className = "",
  }: {
    /** Id of the listbox; option ids derive from it. */
    id: string;
    /** Listed first, in order. */
    branches: Branch[];
    /** Remote-only branches, listed under their own heading. */
    remote?: Branch[];
    query?: string;
    /** Called after the user edits the filter. */
    onQueryInput?: (query: string) => void;
    /** Accessible name of the filter; shown above it with `showLabel`. */
    label: string;
    showLabel?: boolean;
    placeholder?: string;
    combobox?: boolean;
    /** False shows a loading placeholder instead of the list. */
    loaded?: boolean;
    error?: string | null;
    /** Badge on the checked-out branch. */
    currentLabel?: string;
    selected?: (branch: Branch) => boolean;
    disabled?: (branch: Branch) => boolean;
    onPick: (branch: Branch) => void;
    /** Height cap of the scrolling list. */
    listClass?: string;
    /** Classes for the outer element, e.g. margins. */
    class?: string;
  } = $props();

  const matches = $derived(filterBranches(branches, query));
  const remoteMatches = $derived(filterBranches(remote, query));
  const rows = $derived([...matches, ...remoteMatches]);
  let activeIndex = $state(0);

  $effect(() => {
    if (activeIndex >= rows.length) activeIndex = 0;
  });

  function pick(branch: Branch | undefined) {
    if (branch && !disabled(branch)) onPick(branch);
  }

  function onKeydown(event: KeyboardEvent) {
    if (!combobox) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      activeIndex = Math.min(activeIndex + 1, rows.length - 1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      activeIndex = Math.max(activeIndex - 1, 0);
    } else if (event.key === "Enter") {
      const branch = rows[activeIndex];
      if (branch && branch.name !== query) {
        event.preventDefault();
        pick(branch);
      }
    }
  }

  const optionId = (index: number) => `${id}-option-${index}`;
</script>

<div class={className}>
  <label class="block">
    {#if showLabel}
      <span class="text-xs font-medium text-surface-600-400">{label}</span>
    {/if}
    <input
      type={combobox ? "text" : "search"}
      value={query}
      oninput={(event) => {
        query = event.currentTarget.value;
        activeIndex = 0;
        onQueryInput?.(query);
      }}
      onkeydown={onKeydown}
      {placeholder}
      aria-label={showLabel ? undefined : label}
      role={combobox ? "combobox" : undefined}
      aria-expanded={combobox ? true : undefined}
      aria-controls={combobox ? id : undefined}
      aria-autocomplete={combobox ? "list" : undefined}
      aria-activedescendant={combobox && rows.length > 0 ? optionId(activeIndex) : undefined}
      class="w-full field font-mono text-[13px] {showLabel ? "mt-1" : ""}"
      autocomplete="off"
      autocorrect="off"
      autocapitalize="off"
      spellcheck={false}
    >
  </label>
  <div
    {id}
    role="listbox"
    aria-label="Branches"
    class="mt-2 select-none overflow-y-auto rounded-md border border-surface-200-800 bg-surface-50-950 p-1 {listClass}"
  >
    {#if error}
      <p class="px-2 py-2 text-xs text-error-500">{error}</p>
    {:else if !loaded}
      <div class="placeholder m-1 h-8 animate-pulse rounded"></div>
    {:else if rows.length === 0}
      <p class="px-2 py-2 text-xs text-surface-500">
        {branches.length + remote.length === 0 ? "No branches found." : `No branch matches “${query}”.`}
      </p>
    {:else}
      {#each matches as branch, index (branch.name)}
        {@render row(branch, index, false)}
      {/each}
      {#if remoteMatches.length > 0}
        <div class="mt-1 px-2 pt-1 eyebrow text-[10px]">Remote</div>
        {#each remoteMatches as branch, index (branch.name)}
          {@render row(branch, matches.length + index, true)}
        {/each}
      {/if}
    {/if}
  </div>
</div>

{#snippet row(branch: Branch, index: number, grouped: boolean)}
  <button
    type="button"
    role="option"
    id={optionId(index)}
    aria-selected={selected(branch)}
    disabled={disabled(branch)}
    onmouseenter={() => (activeIndex = index)}
    onclick={() => pick(branch)}
    title={branch.isRemote ? `Create a local branch tracking ${branch.name}` : undefined}
    class="menu-row text-sm disabled:opacity-60 {combobox && index === activeIndex ? "preset-tonal" : ""}"
  >
    <GitBranch size={12} class="shrink-0 text-surface-500" />
    <span class="min-w-0 flex-1 truncate font-mono text-[13px]">{branch.name}</span>
    {#if branch.isCurrent}
      <span class="shrink-0 rounded-full bg-primary-500/15 px-1.5 py-0.5 text-[10px] font-medium text-primary-500"
        >{currentLabel}</span
      >
    {:else if branch.isRemote && !grouped}
      <span class="shrink-0 text-[10px] text-surface-500">remote</span>
    {/if}
  </button>
{/snippet}

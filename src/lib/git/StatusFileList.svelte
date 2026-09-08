<script lang="ts">
import { Minus, Plus, Undo2 } from "@lucide/svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import type { StatusFile } from "$lib/types";
import { fileIconFor } from "$lib/utils/fileIcons";
import { changeLabel, groupStatusFiles, type Selection, type StatusRow, type StatusSection } from "./gitStatusFiles";

let {
  files,
  selected = null,
  busy = false,
  onSelect,
  onStage,
  onUnstage,
  onDiscard,
}: {
  files: StatusFile[];
  selected?: Selection | null;
  busy?: boolean;
  onSelect: (selection: Selection) => void;
  onStage: (paths: string[]) => void;
  onUnstage: (paths: string[]) => void;
  /** Tracked paths restore from the index; untracked paths are deleted. */
  onDiscard: (paths: string[], untrackedPaths: string[]) => void;
} = $props();

const groups = $derived(groupStatusFiles(files));

const sections: { key: StatusSection; title: string }[] = [
  { key: "conflicted", title: "Conflicts" },
  { key: "staged", title: "Staged" },
  { key: "unstaged", title: "Changes" },
  { key: "untracked", title: "Untracked" },
];

function basename(path: string): string {
  return path.split("/").pop() ?? path;
}

function isSelected(row: StatusRow): boolean {
  return !!selected && selected.path === row.path && selected.section === row.section;
}

function discardRow(row: StatusRow) {
  if (row.section === "untracked") onDiscard([], [row.path]);
  else onDiscard([row.path], []);
}
</script>

<div class="space-y-3">
  {#each sections as section (section.key)}
    {@const rows = groups[section.key]}
    {#if rows.length > 0}
      <section aria-label={section.title}>
        <div class="flex items-center gap-2 px-1">
          <h3 class="text-[11px] font-semibold uppercase tracking-[0.08em] {section.key === 'conflicted' ? 'text-error-500' : 'text-surface-500'}">
            {section.title}
            <span class="ml-1 rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] font-medium normal-case tracking-normal">{rows.length}</span>
          </h3>
          <span class="flex-1"></span>
          {#if section.key === "staged"}
            <button type="button" disabled={busy} onclick={() => onUnstage(rows.map((row) => row.path))} class="text-[11px] text-surface-500 hover:text-surface-900-100 disabled:opacity-40">Unstage all</button>
          {:else if section.key === "unstaged" || section.key === "untracked" || section.key === "conflicted"}
            <button type="button" disabled={busy} onclick={() => onStage(rows.map((row) => row.path))} class="text-[11px] text-surface-500 hover:text-surface-900-100 disabled:opacity-40">Stage all</button>
          {/if}
        </div>
        <div class="mt-1 space-y-0.5">
          {#each rows as row (row.section + ":" + row.path)}
            {@const icon = fileIconFor(basename(row.path))}
            <div
              class="group/row flex items-center gap-2 rounded px-2 py-1 text-xs {isSelected(row) ? 'preset-tonal' : 'hover:preset-tonal'}"
            >
              <button
                type="button"
                onclick={() => onSelect({ path: row.path, section: row.section })}
                aria-current={isSelected(row) ? "true" : undefined}
                title={row.path}
                class="flex min-w-0 flex-1 items-center gap-2 text-left"
              >
                <icon.icon size={13} class="shrink-0 {icon.class}" />
                <span class="min-w-0 flex-1 truncate">{row.path}</span>
                <span class="w-5 shrink-0 text-center font-mono text-[10px] text-surface-500" title={changeLabel[row.change] ?? row.change}>{row.change}</span>
              </button>
              <div class="flex shrink-0 items-center gap-0.5 opacity-0 transition group-hover/row:opacity-100 focus-within:opacity-100">
                {#if row.section === "staged"}
                  <TooltipButton label="Unstage" onclick={() => onUnstage([row.path])} disabled={busy} aria-label="Unstage {row.path}" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
                    <Minus size={12} />
                  </TooltipButton>
                {:else}
                  <TooltipButton label="Discard changes" onclick={() => discardRow(row)} disabled={busy} aria-label="Discard {row.path}" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500 hover:text-error-500">
                    <Undo2 size={12} />
                  </TooltipButton>
                  <TooltipButton label="Stage" onclick={() => onStage([row.path])} disabled={busy} aria-label="Stage {row.path}" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
                    <Plus size={12} />
                  </TooltipButton>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      </section>
    {/if}
  {/each}
  {#if files.length === 0}
    <p class="rounded-xl border border-dashed border-surface-300-700 px-4 py-6 text-center text-xs text-surface-500">Working tree clean.</p>
  {/if}
</div>

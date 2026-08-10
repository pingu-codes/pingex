<script lang="ts">
import PickerList from "$lib/composer/PickerList.svelte";
import { searchProjectFiles } from "$lib/services/api";
import type { FileHit, Mention } from "$lib/types";
import { fileIconFor, folderIcon } from "$lib/utils/fileIcons";

let {
  cwd,
  query,
  scope = null,
  onPick,
  onClose,
  onCount,
}: {
  cwd: string;
  query: string;
  scope?: HTMLElement | null;
  /** Reports the visible result count to the composer's key handling. */
  onCount?: (count: number) => void;
  onPick: (mention: Mention) => void;
  onClose: () => void;
} = $props();

let results = $state<FileHit[]>([]);
let failed = $state(false);

$effect(() => {
  const currentQuery = query;
  const timer = setTimeout(async () => {
    try {
      const hits = await searchProjectFiles(cwd, currentQuery, 12);
      if (currentQuery === query) {
        results = hits;
        failed = false;
      }
    } catch {
      failed = true;
      results = [];
    }
  }, 120);
  return () => clearTimeout(timer);
});

function pick(hit: FileHit) {
  // Directories keep a trailing slash — the form Codex writes for folder
  // mentions, and what marks the chip as a folder rather than a file.
  const path = `${cwd.replace(/\/$/, "")}/${hit.path}${hit.isDir ? "/" : ""}`;
  onPick({ name: hit.fileName, path });
}
</script>

<PickerList
  items={results}
  label="Attach a project file or folder"
  emptyMessage={`No matching files or folders${query ? ` for “${query}”` : ""}.`}
  error={failed ? "Could not search project files." : null}
  {scope}
  onPick={pick}
  {onClose}
  {onCount}
  key={(hit) => hit.path}
>
  {#snippet row(hit: FileHit)}
    {@const icon = hit.isDir ? folderIcon : fileIconFor(hit.fileName)}
    <icon.icon size={13} class="shrink-0 {icon.class}" />
    <span class="shrink-0 font-medium">{hit.fileName}{hit.isDir ? "/" : ""}</span>
    <span class="min-w-0 flex-1 truncate text-[10px] text-surface-500">{hit.path}{hit.isDir ? "/" : ""}</span>
  {/snippet}
</PickerList>

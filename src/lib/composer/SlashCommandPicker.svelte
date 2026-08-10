<script lang="ts">
import PickerList from "$lib/composer/PickerList.svelte";
import { filterSlashCommands, type SlashCommand } from "$lib/composer/slashCommands";

let {
  query,
  scope = null,
  onPick,
  onClose,
  onCount,
}: {
  query: string;
  scope?: HTMLElement | null;
  /** Reports the visible result count to the composer's key handling. */
  onCount?: (count: number) => void;
  onPick: (command: SlashCommand) => void;
  onClose: () => void;
} = $props();

// The window keydown listener can fire once after the composer has cleared its
// query but before this component is torn down, so `query` is briefly absent.
const results = $derived(filterSlashCommands(query ?? ""));
</script>

<PickerList
  items={results}
  label="Slash commands"
  emptyMessage={`No matching commands for “/${query}”.`}
  {scope}
  {onPick}
  {onClose}
  {onCount}
  key={(command) => command.id}
>
  {#snippet row(command: SlashCommand)}
    <span class="shrink-0 font-medium">/{command.id}</span>
    {#if command.argument}
      <span class="shrink-0 text-[10px] text-surface-400">&lt;{command.argument}&gt;</span>
    {/if}
    <span class="min-w-0 flex-1 truncate text-[10px] text-surface-500">{command.description}</span>
  {/snippet}
</PickerList>

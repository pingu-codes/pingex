<!-- A pull request row with the shared menu, for ActionMenu.test.ts. -->
<script lang="ts">
  import ActionMenu from "./ActionMenu.svelte";
  import type { MenuEntry } from "./actionMenu";

  let { entries, onSelect }: { entries: MenuEntry[]; onSelect: (id: string) => void } = $props();
</script>

<ActionMenu {entries} {onSelect} label="Pull request 415">
  {#snippet row({ rowProps, buttonProps })}
    <div {...rowProps} data-testid="row" class="group/row flex items-center gap-2">
      <span>Claude Code driver over stream-json</span>
      <input aria-label="Note">
      <button {...buttonProps} class="row-action">⋯</button>
    </div>
  {/snippet}
</ActionMenu>

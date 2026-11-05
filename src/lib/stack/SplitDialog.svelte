<!-- Split a PR's commits: the ones up to the picked commit become a new PR
     below it. Resolves { at, name, title }, or null. -->
<script lang="ts">
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import type { CommitLine } from "$lib/types";

  let {
    number,
    headRef,
    commits,
    close,
  }: {
    number: number;
    headRef: string;
    /** The PR's own commits, newest first. */
    commits: CommitLine[];
    close: DialogClose<{ at: string; name: string; title: string }>;
  } = $props();

  // The newest commit can't be the split point: nothing would be left.
  const choices = $derived(commits.slice(1));
  let at = $state<string | null>(null);
  // svelte-ignore state_referenced_locally -- a suggestion only.
  let name = $state(`${headRef}-base`);
  let title = $state("");
  const ready = $derived(at !== null && name.trim().length > 0 && !/\s/.test(name.trim()) && title.trim().length > 0);
</script>

<DialogShell
  title="Split #{number}"
  subtitle="Commits up to the one you pick open as a new PR below #{number}"
  width={480}
  onClose={() => close()}
>
  <form
    id="split-form"
    class="flex flex-col gap-3"
    onsubmit={(event) => {
      event.preventDefault();
      if (ready && at) close({ at, name: name.trim(), title: title.trim() });
    }}
  >
    {#if choices.length === 0}
      <div class="text-xs text-surface-500">#{number} has one commit, so there is nothing to split.</div>
    {:else}
      <fieldset class="panel-card flex max-h-56 flex-col overflow-y-auto p-1">
        <legend class="sr-only">Split after</legend>
        {#each choices as commit (commit.sha)}
          <label class="menu-row cursor-pointer">
            <input type="radio" name="split-at" value={commit.sha} bind:group={at}>
            <span class="font-mono text-[11px] text-surface-500">{commit.sha.slice(0, 7)}</span>
            <span class="grow truncate">{commit.subject}</span>
          </label>
        {/each}
      </fieldset>
    {/if}
    <label class="flex flex-col gap-1 text-xs">
      <span class="font-medium">New git branch</span>
      <input class="field font-mono text-xs" bind:value={name}>
    </label>
    <label class="flex flex-col gap-1 text-xs">
      <span class="font-medium">New PR title</span>
      <input class="field text-xs" bind:value={title}>
    </label>
  </form>
  {#snippet footer()}
    <button type="button" class="btn btn-sm hover:preset-tonal" onclick={() => close()}>Cancel</button>
    <button type="submit" form="split-form" class="btn btn-sm preset-filled-primary-500" disabled={!ready}>
      Split into a new PR
    </button>
  {/snippet}
</DialogShell>

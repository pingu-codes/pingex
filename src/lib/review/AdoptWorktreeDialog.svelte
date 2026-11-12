<script lang="ts">
  /**
   * Check out found the PR's git branch already checked out somewhere. Offers
   * to make that worktree the PR's (Adopt), or to leave it (Cancel). Pingex
   * never switches, updates or removes an adopted worktree. Resolves `true` on
   * Adopt, `null` otherwise.
   */
  import { FolderGit2 } from "@lucide/svelte";
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import { basename } from "$lib/utils/path";

  let {
    number,
    branch,
    path,
    mainClone,
    close,
  }: {
    /** The PR whose git branch it is. */
    number: number;
    branch: string;
    /** Where the branch is checked out. */
    path: string;
    /** `path` is the repository's main clone. */
    mainClone: boolean;
    close: DialogClose<true>;
  } = $props();

  const where = $derived(mainClone ? "the repository's main clone" : basename(path));
</script>

<DialogShell title="Use {where} for #{number}?" width={480} onClose={() => close()}>
  {#snippet icon()}
    <FolderGit2 size={17} class="text-primary-500" />
  {/snippet}

  <p class="mt-3 text-sm leading-6 text-surface-600-400">
    <span class="font-mono">{branch}</span>
    is already checked out in {where}. Pingex can use that as #{number}'s worktree. It stays yours: Pingex never
    switches its branch, brings it up to date or removes it.
  </p>
  <p class="mt-2 rounded-md bg-surface-100-900 px-2 py-1 font-mono text-[11px] text-surface-700-300">{path}</p>

  {#snippet footer()}
    <button type="button" class="btn btn-sm preset-tonal" onclick={() => close()}>Cancel</button>
    <button type="button" class="btn btn-sm preset-filled-primary-500" onclick={() => close(true)}>Adopt</button>
  {/snippet}
</DialogShell>

<!--
  A changed file in full, at any of the PR's commits, read by SHA from the
  fetched PR refs: nothing is checked out.
-->
<script lang="ts">
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import CodeView from "$lib/components/CodeView.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import type { PrUnderReview } from "$lib/review/prUnderReview.svelte";
  import type { PrCommit, PrFileContents } from "$lib/types";
  import { languageForPath } from "$lib/utils/codeLanguage";
  import { shortSha } from "$lib/utils/format";

  let {
    review,
    path,
    headSha,
    commits,
    close,
  }: {
    review: PrUnderReview;
    path: string;
    headSha: string;
    commits: PrCommit[];
    close: DialogClose<void>;
  } = $props();

  // svelte-ignore state_referenced_locally -- opens at the head.
  let sha = $state(headSha);
  // svelte-ignore state_referenced_locally -- a dialog is opened per file.
  const fileQuery = review.fileAt(() => [sha, path]);
  const contents = $derived<PrFileContents | null>(fileQuery.value ?? null);
  const error = $derived(fileQuery.error);
</script>

<DialogShell title={path} subtitle="Whole file, nothing checked out" width={860} onClose={() => close()}>
  <div class="flex flex-col gap-2 text-xs">
    <label class="flex items-center gap-2">
      <span class="text-surface-500">At</span>
      <select class="field font-mono text-[11px]" bind:value={sha} aria-label="Commit">
        <option value={headSha}>Head · {shortSha(headSha)}</option>
        {#each commits.filter((commit) => commit.oid !== headSha) as commit (commit.oid)}
          <option value={commit.oid}>{commit.shortOid} · {commit.headline}</option>
        {/each}
      </select>
    </label>
    {#if error}
      <div class="error-note">{error}</div>
    {:else if !contents}
      <div class="placeholder h-40 animate-pulse rounded-xl"></div>
    {:else if contents.text == null}
      <p class="empty-state">This file is binary.</p>
    {:else}
      <div class="max-h-[60vh] overflow-auto rounded-md border border-surface-200-800">
        <CodeView text={contents.text} language={languageForPath(path)} />
      </div>
      {#if contents.truncated}
        <p class="text-surface-500">Only the start of this file is shown.</p>
      {/if}
    {/if}
  </div>
</DialogShell>

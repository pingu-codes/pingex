<!--
  Which GitHub account this repository's pull requests are read and written
  as, and the way to change it: automatic, one signed-in account for this
  repository only, or the global account (gh's active one, which `gh` in a
  terminal also uses).
-->
<script lang="ts">
  import { CircleUser, TriangleAlert } from "@lucide/svelte";
  import ActionMenu from "$lib/components/ActionMenu.svelte";
  import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";
  import { accountEntries, accountReason, viewerMismatch } from "$lib/inbox/accountMenu";
  import { commands } from "$lib/services/homeRouting";
  import type { RepositoryAccount } from "$lib/types";

  let {
    repoPath,
    viewer,
    syncing,
    onError,
  }: {
    repoPath: string;
    /** Who the last sync read the inbox as. */
    viewer: string | null;
    syncing: boolean;
    onError: (cause: unknown) => void;
  } = $props();

  let account = $state<RepositoryAccount | null>(null);
  let latest = 0;

  async function load() {
    const request = ++latest;
    const next = await commands.reviewRepositoryAccount(repoPath).catch(() => null);
    if (request === latest) account = next;
  }

  $effect(() => {
    void repoPath;
    account = null;
    void load();
  });

  async function select(id: string) {
    const [action, login] = id.split(":");
    try {
      if (action === "auto") await commands.reviewSetRepositoryAccount(repoPath, null);
      else if (action === "pin") await commands.reviewSetRepositoryAccount(repoPath, login);
      else if (action === "global") await commands.reviewSwitchGithubAccount(login);
      else return;
      await load();
    } catch (cause) {
      onError(cause);
    }
  }

  const mismatch = $derived(!!account && !syncing && viewerMismatch(account, viewer));
</script>

{#if account?.login}
  {#if mismatch}
    <TooltipAnchor
      label="This repository should use {account.login}, but GitHub answered as {viewer}"
      multiline
      tabindex={0}
      class="status-badge preset-tonal-warning"
    >
      <TriangleAlert size={10} aria-hidden="true" />Read as {viewer}
    </TooltipAnchor>
  {/if}
  <ActionMenu
    entries={accountEntries(account)}
    label="GitHub account for this repository"
    buttonLabel="GitHub account: {account.login}"
    onSelect={(id) => void select(id)}
  >
    {#snippet row({ buttonProps })}
      <button
        {...buttonProps}
        type="button"
        class="btn h-auto gap-1 rounded-md px-1 py-0.5 text-xs font-normal {account?.source === "pinMissing"
          ? "preset-tonal-error"
          : "hover:preset-tonal"}"
        title={account ? accountReason(account) : undefined}
      >
        <CircleUser size={12} aria-hidden="true" />{account?.login}
      </button>
    {/snippet}
  </ActionMenu>
{/if}

<script lang="ts">
  import { onMount } from "svelte";
  import { commands } from "$lib/services/homeRouting";
  import type { GitHubAccount } from "$lib/types";

  // The inbox before any repository is added: Add repository, and how to
  // connect GitHub so Add repository can list what asks for your review.
  let { onAddRepository }: { onAddRepository: () => void } = $props();

  let account = $state<GitHubAccount | null>(null);
  let checking = $state(true);

  async function check() {
    checking = true;
    account = await commands.reviewGithubAccount().catch(() => null);
    checking = false;
  }

  onMount(() => void check());
</script>

<div class="empty-state flex flex-col items-center gap-2.5">
  <span>No repositories yet. Add one to see its pull requests here.</span>
  <button type="button" class="btn btn-sm preset-filled-primary-500" onclick={onAddRepository}>Add repository</button>
  {#if !checking && account?.login}
    <span>Signed in to GitHub as {account.login}.</span>
  {:else if !checking}
    <span data-testid="connect-github">
      Or connect GitHub to list the ones asking for your review: run <span class="font-mono">gh auth login</span> in a
      terminal, then
      <button type="button" class="anchor" onclick={() => void check()}>check again</button>.
    </span>
  {/if}
</div>

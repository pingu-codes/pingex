<script lang="ts">
  import { repositoryStore } from "$lib/repositories/repositoryStore.svelte";

  // What a project row adds for its repository state. A project no GitHub
  // remote feeds works as projects always have, with "no PRs" in place of a
  // PR count; a cloned repository shows nothing here.
  let { path }: { path: string } = $props();

  const repository = $derived(repositoryStore.byPath[path]);
</script>

{#if repository?.state === "noRemote"}
  <span data-testid="repository-mark:{path}" class="shrink-0 text-[10px] font-normal text-surface-500">no PRs</span>
{/if}

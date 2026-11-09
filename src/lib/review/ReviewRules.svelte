<!--
  The repository's review rules: kinds of Finding agent reviews are told
  to skip. "Skip this kind of finding" on a Finding adds one; here they
  are edited, added and removed. Pingex stores them, never the
  repository's files. Shows nothing for a repository without GitHub.
-->
<script lang="ts">
  import { Plus, X } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { commands } from "$lib/services/homeRouting";
  import { toastFailure } from "$lib/toaster";
  import type { ReviewRule } from "$lib/types";

  let { repoDir }: { repoDir: string } = $props();

  let rules = $state<ReviewRule[] | null>(null);
  let draft = $state("");

  async function load() {
    try {
      rules = await commands.reviewRules(repoDir);
    } catch {
      // No GitHub repository behind this project: no rules to show.
      rules = null;
    }
  }

  onMount(load);

  async function run(work: () => Promise<unknown>) {
    try {
      await work();
      await load();
    } catch (cause) {
      toastFailure(cause);
    }
  }

  function save(rule: ReviewRule, text: string) {
    if (text.trim() === rule.rule) return;
    if (!text.trim()) void run(() => commands.reviewRemoveRule(repoDir, rule.id));
    else void run(() => commands.reviewUpdateRule(repoDir, rule.id, text));
  }

  function add(event: SubmitEvent) {
    event.preventDefault();
    const text = draft.trim();
    if (!text) return;
    draft = "";
    void run(() => commands.reviewAddRule(repoDir, text));
  }
</script>

{#if rules}
  <section aria-label="Review rules" class="mt-5 panel-card p-4">
    <h2 class="text-sm font-medium">Review rules</h2>
    <p class="mt-1 text-xs text-surface-500">
      Agent reviews in this repository are told not to report findings of these kinds.
    </p>
    <ul class="mt-3 flex flex-col gap-1.5">
      {#each rules as rule (rule.id)}
        <li class="flex items-center gap-2">
          <input
            class="field min-w-0 grow text-xs"
            aria-label="Rule"
            value={rule.rule}
            onchange={(event) => save(rule, event.currentTarget.value)}
          >
          <button
            type="button"
            class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
            aria-label="Remove rule {rule.rule}"
            onclick={() => run(() => commands.reviewRemoveRule(repoDir, rule.id))}
          >
            <X size={13} aria-hidden="true" />
          </button>
        </li>
      {:else}
        <li class="text-xs text-surface-500">No rules yet. Dismiss a finding with "Skip this kind" to add one.</li>
      {/each}
    </ul>
    <form class="mt-2 flex items-center gap-2" onsubmit={add}>
      <input
        class="field min-w-0 grow text-xs"
        aria-label="New rule"
        placeholder="A kind of finding to skip"
        bind:value={draft}
      >
      <button type="submit" class="btn btn-sm preset-tonal" disabled={!draft.trim()}><Plus size={13} />Add</button>
    </form>
  </section>
{/if}

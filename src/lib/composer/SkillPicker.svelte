<!--
  The `$` picker: skills Codex would offer for this directory.

  Unlike `@`, the source list is small and changes rarely, so it is fetched once
  per cwd and filtered in memory rather than re-queried per keystroke. Disabled
  skills are hidden — Codex would not load them, so offering one would silently
  do nothing.
-->
<script lang="ts">
import { Sparkles } from "@lucide/svelte";
import PickerList from "$lib/composer/PickerList.svelte";
import { filterSkills, skillHint, skillLabel } from "$lib/composer/skills";
import { listSkillsFor } from "$lib/services/api";
import { skillsStatus } from "$lib/services/codexEvents.svelte";
import type { SkillSummary } from "$lib/types";

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
  onPick: (skill: SkillSummary) => void;
  onClose: () => void;
} = $props();

let all = $state<SkillSummary[]>([]);
let failed = $state(false);

$effect(() => {
  const currentCwd = cwd;
  // Re-read whenever Codex says the skills on disk changed.
  void skillsStatus.nonce;
  let cancelled = false;
  listSkillsFor(currentCwd ? [currentCwd] : [])
    .then((skills) => {
      if (cancelled) return;
      all = skills.filter((skill) => skill.enabled);
      failed = false;
    })
    .catch(() => {
      if (cancelled) return;
      failed = true;
      all = [];
    });
  return () => {
    cancelled = true;
  };
});

const results = $derived(filterSkills(all, query));
</script>

<PickerList
  items={results}
  label="Skills"
  emptyMessage={failed ? "" : `No matching skills${query ? ` for “${query}”` : ""}.`}
  error={failed ? "Could not load skills from Codex." : null}
  {scope}
  {onPick}
  {onClose}
  {onCount}
  key={(skill) => skill.name}
>
  {#snippet row(skill: SkillSummary)}
    <Sparkles size={13} class="shrink-0 text-tertiary-500" />
    <span class="shrink-0 font-medium">{skillLabel(skill)}</span>
    <span class="min-w-0 flex-1 truncate text-[10px] text-surface-500">{skillHint(skill)}</span>
  {/snippet}
</PickerList>

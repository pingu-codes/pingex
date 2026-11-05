<!-- One skill: its summary, controls, and (folded, loaded on demand) its SKILL.md. -->
<script lang="ts">
  import { ChevronRight, FileText, FolderOpen, Sparkles } from "@lucide/svelte";
  import { Collapsible } from "@skeletonlabs/skeleton-svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import ConfirmDeleteButton from "$lib/integrations/ConfirmDeleteButton.svelte";
  import { pluginNote, splitFrontmatter } from "$lib/integrations/integrationsHelpers";
  import { commands } from "$lib/services/homeRouting";
  import type { IntegrationSetting, SkillSummary } from "$lib/types";
  import { errorMessage } from "$lib/utils/errors";
  import { renderMarkdown } from "$lib/utils/markdown";

  let {
    skill,
    setting,
    ownerEnabled = true,
    projectPath = null,
    onToggle,
    onDelete,
    onError,
  }: {
    skill: SkillSummary;
    setting: IntegrationSetting | undefined;
    /** False when the plugin that provides this skill is turned off. */
    ownerEnabled?: boolean;
    /** Set when showing a project's settings; skills are then read-only. */
    projectPath?: string | null;
    onToggle: () => void;
    onDelete: () => Promise<unknown>;
    onError: (message: string) => void;
  } = $props();

  let open = $state(false);
  /** SKILL.md once loaded; null until then. */
  let text = $state<string | null>(null);
  const parsed = $derived(text == null ? null : splitFrontmatter(text));
  const plugin = $derived(pluginNote(setting, ownerEnabled));

  async function setOpen(next: boolean) {
    open = next;
    if (!next || text != null) return;
    try {
      text = await commands.readSkill(skill.path);
    } catch (cause) {
      open = false;
      onError(errorMessage(cause));
    }
  }
</script>

<div id="skill-row-{encodeURIComponent(skill.path)}" class="card border border-surface-200-800 bg-surface-50-950 p-3">
  <Collapsible {open} onOpenChange={(details) => setOpen(details.open)} class="items-stretch gap-0">
    <div class="flex items-center gap-3">
      <Sparkles size={16} class="shrink-0 {skill.enabled ? "text-tertiary-500" : "text-surface-500"}" />
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <span class="truncate font-medium">{skill.displayName || skill.name}</span>
          <span class="shrink-0 rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] text-surface-600-400"
            >{skill.scope}</span
          >
        </div>
        {#if skill.displayName}
          <div class="mt-0.5 truncate font-mono text-[10px] text-surface-400">{skill.name}</div>
        {/if}
      </div>
      <div class="flex shrink-0 items-center gap-1.5">
        <span class="size-1.5 rounded-full {skill.enabled ? "bg-success-500" : "bg-surface-400"}"></span>
        <span class="text-[10px] text-surface-500">{skill.enabled ? "Enabled" : "Disabled"}</span>
      </div>
    </div>
    {#if skill.shortDescription || skill.description}
      <p class="mt-1.5 line-clamp-3 text-[11px] leading-snug text-surface-600-400">
        {skill.shortDescription || skill.description}
      </p>
    {/if}
    <div class="mt-2.5 flex flex-wrap items-center gap-1.5">
      <!-- A disabled plugin overrides its skills' own setting. -->
      {#if !projectPath && ownerEnabled}
        <button type="button" onclick={onToggle} class="btn btn-sm hover:preset-tonal text-xs">
          {skill.enabled ? "Disable" : "Enable"}
        </button>
      {/if}
      {#if plugin}
        <span class="text-xs text-surface-500">{plugin}</span>
      {/if}
      <Collapsible.Trigger class="group btn btn-sm hover:preset-tonal text-xs">
        <ChevronRight size={12} class="transition-transform group-data-[state=open]:rotate-90" />
        View SKILL.md
      </Collapsible.Trigger>
      <TooltipButton
        label="Reveal in Finder"
        type="button"
        onclick={() => commands.revealInFinder(skill.path)}
        aria-label="Reveal {skill.name} in Finder"
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
      >
        <FolderOpen size={13} />
      </TooltipButton>
      <TooltipButton
        label="Open in Zed"
        type="button"
        onclick={() => commands.openInZed(skill.path)}
        aria-label="Open {skill.name} in Zed"
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
      >
        <FileText size={13} />
      </TooltipButton>
      <span class="min-w-0 flex-1 truncate font-mono text-[10px] text-surface-500">{skill.path}</span>
      {#if skill.scope === "user" && !projectPath}
        <ConfirmDeleteButton label="Delete {skill.name}" confirmLabel="Confirm delete" onConfirm={onDelete} />
      {/if}
    </div>
    <Collapsible.Content>
      {#if open}
        <div class="mt-2.5 border-t border-surface-200-800 pt-2.5">
          {#if parsed == null}
            <div class="text-[11px] text-surface-500">Loading SKILL.md…</div>
          {:else}
            <div class="max-h-80 overflow-auto rounded bg-surface-100-900 p-3 text-xs leading-5">
              {#if parsed.meta.length > 0}
                <dl
                  class="mb-2 grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 border-b border-surface-200-800 pb-2 text-[11px]"
                >
                  {#each parsed.meta as entry (entry.key)}
                    <dt class="font-mono text-surface-500">{entry.key}</dt>
                    <dd class="min-w-0 break-words text-surface-600-400">{entry.value}</dd>
                  {/each}
                </dl>
              {/if}
              <div class="prose-md prose-md-compact">{@html renderMarkdown(parsed.body)}</div>
            </div>
          {/if}
        </div>
      {/if}
    </Collapsible.Content>
  </Collapsible>
</div>

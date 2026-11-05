<!--
  One agent Finding inline at its line: severity, explanation, evidence, and
  "draft, only you can see this" until the reviewer adds it to the review.
  Add to my review takes the Finding's text (or an edit of it) into the
  pending review; Dismiss asks why, and the next agent review hears it.
  A Finding merged into a human comment shows as one line.
-->
<script lang="ts">
  import { ChevronDown, Sparkles } from "@lucide/svelte";
  import { openDialog } from "$lib/app/dialogs.svelte";
  import ActionMenu from "$lib/components/ActionMenu.svelte";
  import type { MenuEntry } from "$lib/components/actionMenu";
  import FindingTextDialog from "$lib/review/FindingTextDialog.svelte";
  import { findingBody } from "$lib/review/findings.svelte";
  import type { DismissReason, Finding } from "$lib/types";

  let {
    finding,
    agentLabel,
    repoName,
    added,
    addedBody = null,
    sameAs = null,
    onAdd,
    onRemove,
    onAskWhy,
    onDismiss,
    onSkipKind,
    compact = false,
  }: {
    finding: Finding;
    /** Who found it: the harness's name. */
    agentLabel: string;
    repoName: string;
    /** In the pending review. */
    added: boolean;
    /** The text it posts, once added. */
    addedBody?: string | null;
    /** "tomasz's loader.ts:6", for a Finding merged into that comment. */
    sameAs?: string | null;
    onAdd: (body?: string) => void;
    onRemove: () => void;
    onAskWhy?: () => void;
    onDismiss: (reason: DismissReason, note?: string | null) => void;
    /** Save "Skip this kind of finding" as a repository review rule. */
    onSkipKind: (rule: string) => void;
    /** Outside a diff (the summary panel): no indent to the code column. */
    compact?: boolean;
  } = $props();

  const place = $derived(compact ? "mx-0! my-1.5!" : "");

  const SEVERITY = {
    high: { label: "High", tone: "preset-tonal-error" },
    medium: { label: "Medium", tone: "preset-tonal-warning" },
    low: { label: "Low", tone: "preset-tonal" },
  } as const;

  let editing = $state(false);
  let draft = $state("");
  const severity = $derived(SEVERITY[finding.severity]);

  const dismissMenu: MenuEntry[] = $derived([
    { id: "notAProblem", label: "Not a problem", shortcut: "1" },
    { id: "alreadyRaised", label: "Already raised in a comment", shortcut: "2" },
    { id: "outOfScope", label: "Out of scope for this PR", shortcut: "3" },
    { id: "wrong", label: "Wrong, tell the agent why…" },
    { kind: "separator" },
    { id: "skip", label: `Skip this kind of finding in ${repoName}…` },
  ]);

  function edit() {
    draft = addedBody ?? findingBody(finding);
    editing = true;
  }

  function save() {
    onAdd(draft);
    editing = false;
  }

  async function dismissWith(id: string) {
    if (id === "wrong") {
      const note = await openDialog(FindingTextDialog, {
        title: "Tell the agent why",
        body: "The next agent review on this PR reads this, so it doesn't raise the same thing again.",
        label: "Why it's wrong",
        confirmLabel: "Dismiss",
      });
      if (note) onDismiss("wrong", note);
      return;
    }
    if (id === "skip") {
      const rule = await openDialog(FindingTextDialog, {
        title: `Skip this kind of finding in ${repoName}?`,
        body: `Agent reviews in ${repoName} will be told not to report findings like this. You can edit the rule in the repository's settings.`,
        label: "Rule",
        initial: finding.title,
        confirmLabel: "Save rule",
      });
      if (rule) {
        onSkipKind(rule);
        onDismiss("notAProblem");
      }
      return;
    }
    onDismiss(id as DismissReason);
  }
</script>

{#if finding.state === "merged"}
  <div class="finding-card {place} flex items-center gap-2 px-2.5 py-1.5 text-surface-500" role="note">
    <Sparkles size={12} aria-hidden="true" class="text-primary-500" />
    <span class="truncate"
      >{agentLabel}
      found this too: {finding.title}. Same as {sameAs ?? "an existing"} comment.</span
    >
  </div>
{:else}
  <article class="finding-card {place}" aria-label="Agent finding: {finding.title}">
    {#if compact}
      <div class="px-2.5 pt-2 font-mono text-[11px] text-surface-500">{finding.path}:{finding.line}</div>
    {/if}
    <div class="finding-head">
      <Sparkles size={12} aria-hidden="true" />
      <b class="font-semibold">Agent finding</b>
      <span>{agentLabel} · {added ? "added to your review" : "draft, only you can see this"}</span>
      <span class="grow"></span>
      <span class="status-badge {severity.tone}">{severity.label}</span>
    </div>
    <div class="flex flex-col gap-2 p-2.5">
      {#if editing}
        <label class="flex flex-col gap-1">
          <span class="sr-only">Comment text</span>
          <textarea class="field resize-y text-[13px]" rows="4" bind:value={draft}></textarea>
        </label>
        <div class="flex gap-1.5">
          <button type="button" class="btn btn-sm preset-tonal text-[11px]" disabled={!draft.trim()} onclick={save}>
            {added ? "Save" : "Add to my review"}
          </button>
          <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" onclick={() => (editing = false)}>
            Cancel
          </button>
        </div>
      {:else}
        {#if finding.title}
          <div class="text-[13px] leading-5 font-medium">{finding.title}</div>
        {/if}
        <div class="text-[13px] leading-5 whitespace-pre-wrap">{added && addedBody ? addedBody : finding.body}</div>
        {#if finding.evidence}
          <div class="finding-evidence">{finding.evidence}</div>
        {/if}
        <div class="flex flex-wrap items-center gap-1.5">
          {#if added}
            <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" onclick={edit}>Edit</button>
            <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" onclick={onRemove}>
              Remove from review
            </button>
          {:else}
            <button type="button" class="btn btn-sm preset-tonal text-[11px]" onclick={() => onAdd()}>
              Add to my review
            </button>
            <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" onclick={edit}>Edit</button>
          {/if}
          {#if onAskWhy}
            <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" onclick={onAskWhy}>
              Ask agent why
            </button>
          {/if}
          <span class="grow"></span>
          <ActionMenu
            entries={dismissMenu}
            label="Dismiss reason"
            onSelect={(id) => void dismissWith(id)}
            buttonLabel="Dismiss"
          >
            {#snippet row({ rowProps, buttonProps })}
              <span {...rowProps}>
                <button
                  {...buttonProps}
                  type="button"
                  class="btn btn-sm hover:preset-tonal text-[11px] text-surface-500"
                >
                  Dismiss<ChevronDown size={12} aria-hidden="true" />
                </button>
              </span>
            {/snippet}
          </ActionMenu>
        </div>
      {/if}
    </div>
  </article>
{/if}

<script lang="ts" module>
  import type { AgentReviewRequest } from "$lib/types";

  /** Start an agent review as asked, or open the PR without one. */
  export type StartReviewResult = { kind: "start"; request: AgentReviewRequest } | { kind: "open" };
</script>

<script lang="ts">
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import { DEFAULT_FOCUS, defaultReviewHarness, installedHarnesses } from "$lib/review/agentReviews.svelte";
  import type { CheckoutTarget, FocusArea, HarnessKind, InboxPr } from "$lib/types";
  import { harnessLabel } from "$lib/utils/capabilities";
  /**
   * Start review on a checked-out PR: the harness, focus areas and how
   * Findings come back. The agent works in the PR's worktree, where it can
   * build, run tests and measure; the dialog says where that is.
   */
  import { basename } from "$lib/utils/path";

  let {
    pr,
    target,
    worktreePath,
    harnesses = installedHarnesses(),
    close,
  }: {
    pr: InboxPr;
    /** The PR as what the review runs on. */
    target: CheckoutTarget;
    /** The PR's worktree, where the agent works. */
    worktreePath: string;
    /** The harnesses to offer: what this Home has installed. */
    harnesses?: HarnessKind[];
    close: DialogClose<StartReviewResult>;
  } = $props();

  const FOCUS: { value: FocusArea; label: string }[] = [
    { value: "correctness", label: "Correctness" },
    { value: "tests", label: "Tests" },
    { value: "accessibility", label: "Accessibility" },
    { value: "performance", label: "Performance" },
    { value: "agentInstructions", label: "Follow the repo's agent instructions" },
    { value: "runTests", label: "Run tests" },
  ];

  // svelte-ignore state_referenced_locally -- the starting choice only.
  let harness = $state<HarnessKind>(defaultReviewHarness(harnesses));
  let focus = $state<FocusArea[]>([...DEFAULT_FOCUS]);
  let keepDrafts = $state(true);
  let rerun = $state(false);

  const subtitle = $derived(`${pr.author} · ${pr.title} · +${pr.additions} −${pr.deletions}`);

  function toggleFocus(area: FocusArea) {
    if (focus.includes(area)) {
      focus = focus.filter((entry) => entry !== area);
      return;
    }
    focus = FOCUS.map((option) => option.value).filter((value) => value === area || focus.includes(value));
  }

  function start() {
    close({
      kind: "start",
      request: {
        targets: [target],
        harness,
        focus,
        keepDrafts,
        rerunOnNewCommits: rerun,
      },
    });
  }
</script>

<DialogShell title="Review #{pr.number}" {subtitle} width={540} onClose={() => close()}>
  <div class="mt-4 flex flex-col gap-4 text-xs">
    <p class="text-surface-500">
      Works in the PR's worktree,
      <span class="font-mono" title={worktreePath}>{basename(worktreePath)}</span>, where the agent can build, run tests
      and measure.
      {#if !target.mine}
        Nothing reaches their branch until you push.
      {/if}
    </p>

    <div class="flex flex-col gap-1.5">
      <div class="eyebrow">Agent review</div>
      <div class="flex flex-wrap items-center gap-2">
        <Segmented
          label="Harness"
          value={harness}
          options={harnesses.map((kind) => ({ value: kind, label: harnessLabel(kind) }))}
          onChange={(next) => (harness = next)}
        />
        <span class="text-surface-500">Runs as its own thread, filed under #{pr.number}</span>
      </div>
      <div class="flex flex-wrap gap-1.5">
        {#each FOCUS as option (option.value)}
          <button
            type="button"
            class={focus.includes(option.value) ? "pill-active" : "pill"}
            aria-pressed={focus.includes(option.value)}
            onclick={() => toggleFocus(option.value)}
          >
            {option.label}
          </button>
        {/each}
      </div>
    </div>

    <div class="panel-card">
      <div class="flex items-center justify-between gap-4 px-3 py-2.5">
        <div>
          <div class="font-medium">Keep findings as drafts</div>
          <div class="text-surface-500">Nothing reaches GitHub until you add it to your review.</div>
        </div>
        <Toggle checked={keepDrafts} label="Keep findings as drafts" onChange={(on) => (keepDrafts = on)} />
      </div>
      <div class="h-px bg-surface-200-800"></div>
      <div class="flex items-center justify-between gap-4 px-3 py-2.5">
        <div>
          <div class="font-medium">Re-run when new commits land</div>
          <div class="text-surface-500">Reviews only the commits since the last run.</div>
        </div>
        <Toggle checked={rerun} label="Re-run when new commits land" onChange={(on) => (rerun = on)} />
      </div>
    </div>
  </div>

  {#snippet footer()}
    <button type="button" class="btn btn-sm hover:preset-tonal" onclick={() => close()}>Cancel</button>
    <button type="button" class="btn btn-sm preset-tonal" onclick={() => close({ kind: "open" })}>
      Open without agent
    </button>
    <button type="button" class="btn btn-sm preset-filled-primary-500" onclick={start}>Start review</button>
  {/snippet}
</DialogShell>

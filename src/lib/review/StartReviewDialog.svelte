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
  import type { CheckoutStep, CheckoutTarget, FocusArea, HarnessKind, InboxPr } from "$lib/types";
  import { can, harnessLabel } from "$lib/utils/capabilities";
  /**
   * Start review on someone else's PR: the code step (no checkout, review
   * checkout, worktree on their git branch), the harness, focus areas and
   * how Findings come back. "Run tests" needs a review checkout and switches
   * the code step to one; a harness without a read-only sandbox can only
   * review in a worktree, so it can't run tests. Capabilities come from
   * `can()`, never the harness kind.
   */
  import { shortSha } from "$lib/utils/format";

  let {
    pr,
    target,
    worktreePath,
    setupCommand = null,
    harnesses = installedHarnesses(),
    refusedCheckout = null,
    close,
  }: {
    pr: InboxPr;
    /** The PR as what the review runs on. */
    target: CheckoutTarget;
    /** Where the PR's worktree goes, or already is. */
    worktreePath: string;
    /** The repository's setup command, run once in a new checkout. */
    setupCommand?: string | null;
    /** The harnesses to offer: what this Home has installed. */
    harnesses?: HarnessKind[];
    /** Why the repository won't make a worktree, if it won't. */
    refusedCheckout?: string | null;
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
  let step = $state<CheckoutStep>("noCheckout");
  let focus = $state<FocusArea[]>([...DEFAULT_FOCUS]);
  let keepDrafts = $state(true);
  let rerun = $state(false);

  const readOnly = $derived(can(harness, "readOnlySandbox"));
  const runTests = $derived(focus.includes("runTests"));
  const shortHead = $derived(shortSha(pr.headSha));
  const subtitle = $derived(`${pr.author} · ${pr.title} · +${pr.additions} −${pr.deletions}`);

  /** Steps the harness can work at: No checkout runs in the user's own clone,
   *  so it needs a read-only sandbox. */
  function stepAllowed(candidate: CheckoutStep): boolean {
    return candidate === "worktree" ? !refusedCheckout : readOnly;
  }

  function chooseHarness(next: HarnessKind) {
    harness = next;
    if (!stepAllowed(step)) {
      step = "worktree";
    }
  }

  function chooseStep(next: CheckoutStep) {
    step = next;
    // The dialog never keeps asking for what the step can't do.
    if (next === "noCheckout") focus = focus.filter((area) => area !== "runTests");
  }

  function toggleFocus(area: FocusArea) {
    if (focus.includes(area)) {
      focus = focus.filter((entry) => entry !== area);
      return;
    }
    focus = FOCUS.map((option) => option.value).filter((value) => value === area || focus.includes(value));
    if (area === "runTests" && step === "noCheckout") step = "worktree";
  }

  function start() {
    close({
      kind: "start",
      request: {
        targets: [target],
        harness,
        step,
        focus,
        keepDrafts,
        rerunOnNewCommits: rerun,
      },
    });
  }

  const runTestsTitle = $derived(refusedCheckout ?? undefined);
</script>

<DialogShell title="Review #{pr.number}" {subtitle} width={540} onClose={() => close()}>
  <div class="mt-4 flex flex-col gap-4 text-xs">
    <div class="flex flex-col gap-1.5">
      <div class="eyebrow">Code</div>
      <div class="panel-card p-1.5" role="radiogroup" aria-label="Code">
        {@render stepRow("noCheckout")}
        {@render stepRow("worktree")}
      </div>
    </div>

    <div class="flex flex-col gap-1.5">
      <div class="eyebrow">Agent review</div>
      <div class="flex flex-wrap items-center gap-2">
        <Segmented
          label="Harness"
          value={harness}
          options={harnesses.map((kind) => ({ value: kind, label: harnessLabel(kind) }))}
          onChange={chooseHarness}
        />
        <span class="text-surface-500">Runs as its own thread, filed under #{pr.number}</span>
      </div>
      <div class="flex flex-wrap gap-1.5">
        {#each FOCUS as option (option.value)}
          {@const disabled = option.value === "runTests" && !!refusedCheckout}
          <button
            type="button"
            class={focus.includes(option.value) ? "pill-active" : "pill"}
            aria-pressed={focus.includes(option.value)}
            {disabled}
            title={option.value === "runTests" ? runTestsTitle : undefined}
            onclick={() => toggleFocus(option.value)}
          >
            {option.label}
          </button>
        {/each}
      </div>
      <div class="text-surface-500">
        {#if !readOnly}
          {harnessLabel(harness)}
          has no read-only sandbox, so it reviews in a worktree.
        {:else}
          Run tests needs a worktree. Picking it switches Code to Worktree.
        {/if}
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

{#snippet stepRow(value: CheckoutStep)}
  {@const allowed = stepAllowed(value)}
  <label
    class="menu-row items-start {step === value ? "bg-surface-200-800" : ""} {allowed
      ? "cursor-pointer"
      : "opacity-50"}"
  >
    <input
      type="radio"
      name="start-review-code"
      class="mt-0.5"
      checked={step === value}
      disabled={!allowed}
      onchange={() => chooseStep(value)}
    >
    <span class="flex grow flex-col">
      {#if value === "noCheckout"}
        <span>No checkout</span>
        <span class="text-surface-500"
          >Diff review. The agent reads the patch and files at <span class="font-mono">{shortHead}</span>, and runs
          nothing.</span
        >
      {:else}
        <span>Worktree</span>
        <span class="text-surface-500">
          {#if refusedCheckout}
            {refusedCheckout}
          {:else}
            At the PR head in
            <span class="font-mono">{worktreePath}</span>
            {#if setupCommand}
              , then <span class="font-mono">{setupCommand}</span>
            {/if}
            . The agent can build, run tests and measure.
            {#if !target.mine}
              Nothing reaches their branch until you push.
            {/if}
          {/if}
        </span>
      {/if}
    </span>
    {#if value === "noCheckout"}
      <span class="status-badge preset-tonal">Nothing on disk</span>
    {/if}
  </label>
{/snippet}

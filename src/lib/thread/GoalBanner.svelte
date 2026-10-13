<script lang="ts">
import { Gauge, Pause, Pencil, Play, Target, X } from "@lucide/svelte";
import { Progress } from "@skeletonlabs/skeleton-svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import type { ThreadGoal } from "$lib/types";
import { formatTokensShort } from "$lib/utils/format";

let {
  goal,
  onEditBudget,
  onEdit,
  onToggle,
  onClear,
}: {
  goal: ThreadGoal;
  /** Cap, raise or lift the token budget. */
  onEditBudget: () => void;
  /** Move the objective into the composer, whose send confirms it. */
  onEdit: () => void;
  /** Pause a running goal, or resume a stalled one. */
  onToggle: () => void;
  onClear: () => void;
} = $props();

const STATUS_LABEL: Record<string, string> = {
  active: "active",
  paused: "paused",
  blocked: "blocked",
  usageLimited: "waiting on usage limit",
  budgetLimited: "budget exhausted",
  complete: "complete",
};

/** Badge preset for each goal status, so the banner shows state at a glance. */
const STATUS_PRESET: Record<string, string> = {
  active: "preset-filled-primary-500",
  paused: "preset-tonal-surface",
  blocked: "preset-tonal-error",
  usageLimited: "preset-tonal-warning",
  budgetLimited: "preset-tonal-error",
  complete: "preset-tonal-success",
};

const iconButton = "shrink-0 opacity-60 hover:opacity-100";
</script>

<div class="mx-auto w-full max-w-3xl px-6">
  <div
    class="mb-2 flex items-center gap-2 rounded-lg border border-surface-200-800 bg-surface-100-900 px-3 py-1.5 text-xs"
    data-testid="goal-banner"
    title={goal.tokenBudget ? `${goal.tokensUsed.toLocaleString()} of ${goal.tokenBudget.toLocaleString()} tokens used` : undefined}
  >
    <Target size={12} class="shrink-0 {goal.status === 'active' ? 'text-primary-500' : 'text-surface-500'}" />
    <span class="shrink-0 font-medium text-surface-500">Goal</span>
    <span
      class="badge shrink-0 px-1.5 py-0 text-[10px] {STATUS_PRESET[goal.status] ?? 'preset-tonal-surface'}"
      data-testid="goal-status"
    >
      {STATUS_LABEL[goal.status] ?? goal.status}
    </span>
    <span class="min-w-0 flex-1 truncate" title={goal.objective}>{goal.objective}</span>
    {#if goal.tokenBudget}
      {@const used = Math.min(goal.tokensUsed, goal.tokenBudget)}
      <span class="flex shrink-0 items-center gap-1.5 text-[10px] tabular-nums text-surface-500" data-testid="goal-budget">
        <!-- Clamped: spend can overshoot the cap, and Progress refuses a value past its max. -->
        <Progress value={used} max={goal.tokenBudget} class="w-16 gap-0">
          <Progress.Track aria-label="Goal budget used" class="h-1.5 rounded-full bg-surface-200-800">
            <Progress.Range
              class="rounded-full {goal.status === 'budgetLimited'
                ? 'bg-error-500'
                : used >= goal.tokenBudget * 0.9
                  ? 'bg-warning-500'
                  : 'bg-primary-500'}"
            />
          </Progress.Track>
        </Progress>
        {formatTokensShort(goal.tokensUsed)} / {formatTokensShort(goal.tokenBudget)}
      </span>
    {/if}
    {#if goal.status === "budgetLimited"}
      <button type="button" class="btn btn-sm shrink-0 py-0 text-[10px] preset-tonal-error" onclick={onEditBudget}>
        Raise budget
      </button>
    {/if}
    <TooltipButton label={goal.tokenBudget ? "Change goal budget" : "Set goal budget"} onclick={onEditBudget} class={iconButton}>
      <Gauge size={12} />
    </TooltipButton>
    <TooltipButton label="Edit goal" onclick={onEdit} class={iconButton}>
      <Pencil size={12} />
    </TooltipButton>
    {#if goal.status !== "complete"}
      <TooltipButton label={goal.status === "active" ? "Pause goal" : "Resume goal"} onclick={onToggle} class={iconButton}>
        {#if goal.status === "active"}<Pause size={12} />{:else}<Play size={12} />{/if}
      </TooltipButton>
    {/if}
    <TooltipButton label="Clear goal" onclick={onClear} class={iconButton}>
      <X size={12} />
    </TooltipButton>
  </div>
</div>

<!-- Agent reviews queued and running on this repository's PRs. -->
<script lang="ts">
  import type { AgentReviewRow } from "$lib/inbox/agentReviews";
  import { harnessLabel } from "$lib/utils/capabilities";

  let { reviews }: { reviews: AgentReviewRow[] } = $props();
</script>

<section aria-label="Agent reviews" class="flex flex-col gap-3">
  <h2 class="eyebrow mt-2">Agent reviews</h2>
  <div class="panel-card flex flex-col gap-2 px-3 py-2.5 text-xs">
    {#each reviews as review (review.id)}
      <div class="flex items-center gap-2">
        <span class="font-mono text-[11px] text-surface-500">#{review.number}</span>
        <span class="grow">{harnessLabel(review.harness)}</span>
        <span class="status-badge {review.status === "running" ? "preset-tonal-primary" : "preset-tonal"}"
          >{review.status === "running" ? "Running" : "Queued"}</span
        >
      </div>
    {:else}
      <div class="text-surface-500">No agent reviews running.</div>
    {/each}
    <div class="text-surface-500">
      Diff reviews need no checkout. Reviews that run tests get a review checkout first. Findings stay as drafts until
      you post them.
    </div>
  </div>
</section>

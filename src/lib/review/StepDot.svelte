<script lang="ts">
  // The canvas's checkout-step dot (`lvl` on the Model board): No checkout is
  // a muted ring, Review checkout a dashed accent ring, Worktree filled.
  import type { CheckoutStep } from "$lib/types";

  let { step }: { step: CheckoutStep } = $props();
</script>

<span
  aria-hidden="true"
  class="inline-block size-2.5 shrink-0 rounded-full {step === "noCheckout"
    ? "border-2 border-surface-400-600"
    : step === "reviewCheckout"
      ? "border-2 border-dashed border-primary-500"
      : "bg-success-500"}"
></span>

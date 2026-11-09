<script lang="ts">
  // The canvas's checkout-step dot (`lvl` on the Model board): No checkout is
  // a muted ring, Worktree filled.
  import type { CheckoutStep } from "$lib/types";

  let { step }: { step: CheckoutStep } = $props();
</script>

<span
  aria-hidden="true"
  class="inline-block size-2.5 shrink-0 rounded-full {step === "noCheckout"
    ? "border-2 border-surface-400-600"
    : "bg-success-500"}"
></span>

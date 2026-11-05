<script lang="ts">
  /**
   * A PR's checkout step and the two quick actions that run code: Run
   * locally and Open in editor. On a PR below a review checkout their
   * tooltips name the step they create; pressing one creates it (asking
   * first per the repository's policy), with an Undo toast.
   */
  import { Code2, Play } from "@lucide/svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import { STEP_LABELS, stepHint } from "$lib/review/actions";
  import { stepUpFor } from "$lib/review/checkoutSteps";
  import StepDot from "$lib/review/StepDot.svelte";
  import { commands } from "$lib/services/homeRouting";
  import { toastError } from "$lib/toaster";
  import type { CheckoutTarget, DeskAction, PrCheckout } from "$lib/types";
  import { errorMessage } from "$lib/utils/errors";

  let {
    repoDir,
    repoName,
    target,
    onRunLocally,
  }: {
    repoDir: string;
    repoName: string;
    target: CheckoutTarget;
    /** Run the PR in its checkout: today, a new thread there. */
    onRunLocally: (path: string) => void;
  } = $props();

  let checkout = $state<PrCheckout | null>(null);
  let busy = $state(false);
  const step = $derived(checkout?.step ?? "noCheckout");

  $effect(() => {
    const asked = target;
    void commands
      .reviewCheckoutState(repoDir, asked)
      .then((state) => {
        if (asked === target) checkout = state;
      })
      .catch(() => {});
  });

  const tooltip = (action: DeskAction, label: string) => {
    const hint = stepHint(step, action);
    return hint ? `${label} · ${hint}` : label;
  };

  async function run(action: DeskAction) {
    busy = true;
    try {
      const next = await stepUpFor({ repoDir, repoName, target, action });
      if (!next?.path) return;
      checkout = next;
      if (action === "runLocally") onRunLocally(next.path);
      else await commands.openInZed(next.path);
    } catch (cause) {
      toastError(errorMessage(cause));
    } finally {
      busy = false;
    }
  }
</script>

<div class="flex items-center gap-1.5" data-testid="checkout-actions">
  <span class="badge inline-flex items-center gap-1.5 preset-tonal text-[10px]" data-testid="checkout-step">
    <StepDot {step} />
    {STEP_LABELS[step]}
  </span>
  <TooltipButton
    label={tooltip("runLocally", "Run locally")}
    class="btn btn-sm preset-tonal"
    disabled={busy}
    onclick={() => run("runLocally")}
  >
    <Play size={13} />
    <span>Run locally</span>
  </TooltipButton>
  <TooltipButton
    label={tooltip("openInEditor", "Open in editor")}
    class="btn btn-sm preset-tonal"
    disabled={busy}
    onclick={() => run("openInEditor")}
  >
    <Code2 size={13} />
    <span>Open in editor</span>
  </TooltipButton>
</div>

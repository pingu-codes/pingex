<script lang="ts">
  /**
   * A PR thread's git-branch chip: the git branch and the commit the thread
   * started on, and how far the branch has moved since. Hovering shows the
   * detail (after 500ms, no actions inside); its menu checks out the start
   * or the tip, compares them, or copies the branch name or commit.
   */
  import { GitBranch } from "@lucide/svelte";
  import { openDialog } from "$lib/app/dialogs.svelte";
  import ActionMenu from "$lib/components/ActionMenu.svelte";
  import type { MenuEntry } from "$lib/components/actionMenu";
  import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";
  import CompareCommitsDialog from "$lib/review/CompareCommitsDialog.svelte";
  import { commands } from "$lib/services/homeRouting";
  import { toastError, toaster } from "$lib/toaster";
  import type { BranchProgress, PrLink } from "$lib/types";
  import { errorMessage } from "$lib/utils/errors";

  let { link }: { link: PrLink } = $props();

  let progress = $state<BranchProgress | null>(null);

  async function refresh() {
    try {
      progress = await commands.reviewBranchProgress(link.threadId);
    } catch {
      progress = null;
    }
  }

  $effect(() => {
    void link.threadId;
    void refresh();
  });

  const start = $derived(link.startSha ?? "");
  const short = (sha: string | null | undefined) => (sha ?? "").slice(0, 7);
  const moved = $derived.by(() => {
    if (!progress?.tipSha) return "the branch is gone";
    if (progress.behind > 0) return `the branch was rewritten (${progress.ahead} new, ${progress.behind} dropped)`;
    if (progress.ahead === 0) return "the branch has not moved";
    return `the branch is now ${progress.ahead} commit${progress.ahead === 1 ? "" : "s"} ahead`;
  });
  const summary = $derived(`Thread started at ${short(start)} · ${moved}`);
  const noCheckout = $derived(link.step === "noCheckout" || !link.checkoutPath);
  const checkoutTitle = "This thread started without a checkout";

  const entries = $derived<MenuEntry[]>([
    { id: "summary", label: summary, disabled: true },
    { kind: "separator" },
    {
      id: "start",
      label: "Check out start commit",
      disabled: noCheckout,
      title: noCheckout ? checkoutTitle : undefined,
    },
    { id: "tip", label: "Check out branch tip", disabled: noCheckout || !progress?.tipSha },
    { id: "compare", label: "Compare start to tip", disabled: !progress?.tipSha || progress.tipSha === start },
    { kind: "separator" },
    { id: "copyBranch", label: "Copy branch name", disabled: !link.gitBranch },
    { id: "copyCommit", label: "Copy commit" },
  ]);

  async function run(id: string) {
    try {
      if (id === "start" || id === "tip") {
        await commands.reviewCheckOutThreadCommit(link.threadId, id === "start");
        toaster.success({ title: id === "start" ? `Checked out ${short(start)}` : `Checked out ${link.gitBranch}` });
        await refresh();
      } else if (id === "compare" && progress?.tipSha) {
        await openDialog(CompareCommitsDialog, { repoDir: link.repoKey, base: start, head: progress.tipSha });
      } else if (id === "copyBranch" && link.gitBranch) {
        await commands.copyText(link.gitBranch);
      } else if (id === "copyCommit") {
        await commands.copyText(start);
      }
    } catch (cause) {
      toastError(errorMessage(cause));
    }
  }
</script>

<ActionMenu {entries} label="Git branch {link.gitBranch ?? ""}" onSelect={run} buttonLabel="Git branch menu">
  {#snippet row({ rowProps, buttonProps })}
    <span {...rowProps} class="inline-flex min-w-0" data-testid="thread-branch-chip">
      <TooltipAnchor label={summary} openDelay={500}>
        <button
          {...buttonProps}
          type="button"
          class="inline-flex min-w-0 items-center gap-1 rounded-full px-2 py-0.5 font-mono text-[10px] text-surface-600-400 preset-tonal hover:text-surface-800-200"
        >
          <GitBranch size={10} class="shrink-0" />
          <span class="max-w-[8rem] truncate">{link.gitBranch ?? "detached"}</span>
          <span class="text-surface-500">@ {short(start)}</span>
          {#if progress && progress.ahead > 0}
            <span class="text-primary-500">+{progress.ahead}</span>
          {/if}
        </button>
      </TooltipAnchor>
    </span>
  {/snippet}
</ActionMenu>

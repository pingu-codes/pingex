<script lang="ts">
  import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";
  import type { WorktreePurpose } from "$lib/types";
  import { worktreeKind } from "$lib/worktrees/worktreeKind";

  type Props = {
    /** `Project.kind`: "folder" | "worktree" | "multiProject". */
    kind: string;
    purpose?: WorktreePurpose | null;
    size?: number;
    /** Muted for rows that are hidden, archived or not added. */
    muted?: boolean;
    /** Said before the icon's own explanation, such as "Not added to the sidebar". */
    note?: string;
    class?: string;
  };

  let { kind, purpose = null, size = 15, muted = false, note, class: className = "" }: Props = $props();

  const shown = $derived(worktreeKind(kind, purpose));
  const Icon = $derived(shown.icon);
  const tooltip = $derived([note, shown.tooltip].filter(Boolean).join("\n"));
  const tone = $derived(muted || kind === "folder" ? "text-surface-500" : "text-primary-500");
</script>

{#if tooltip}
  <TooltipAnchor
    label={tooltip}
    multiline={true}
    role="img"
    aria-label={tooltip}
    class="grid shrink-0 place-items-center {className}"
    data-testid="project-icon"
    data-purpose={purpose?.kind ?? kind}
  >
    <Icon class={tone} {size} strokeWidth={1.7} />
  </TooltipAnchor>
{:else}
  <Icon class="shrink-0 {tone} {className}" {size} strokeWidth={1.7} />
{/if}

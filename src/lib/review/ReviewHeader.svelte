<!--
  The review page's topbar: where the PR is and what it is, the PR's actions,
  the tabs, and on the Files tab how the files show. `actions` goes first among
  the actions. It scrolls away with the page, which measures it through `height`.
-->
<script lang="ts">
  import { ChevronLeft, Ellipsis, Keyboard, PanelRight } from "@lucide/svelte";
  import { Popover, Portal } from "@skeletonlabs/skeleton-svelte";
  import type { Snippet } from "svelte";
  import ActionMenu from "$lib/components/ActionMenu.svelte";
  import Segmented, { type SegmentedOption } from "$lib/components/Segmented.svelte";
  import type { People } from "$lib/inbox/inbox";
  import LoginAvatar from "$lib/inbox/LoginAvatar.svelte";
  import { prMenuEntries } from "$lib/review/prActions";
  import { REVIEW_SHORTCUTS, type ReviewFilter, type ReviewTab } from "$lib/review/reviewPage";
  import { commands } from "$lib/services/homeRouting";
  import type { PrDetail } from "$lib/types";

  type Rect = { x: number; y: number; width: number; height: number };

  let {
    height = $bindable(0),
    helpOpen = $bindable(false),
    onBack,
    backLabel,
    repoName,
    detail,
    number,
    people,
    drift,
    actions,
    onPrMenu,
    tab,
    tabs,
    onTabChange,
    filter,
    filters,
    onFilterChange,
    changes,
    onShowChanges,
    viewedCount,
    fileCount,
    onExpandAll,
    onCollapseAll,
    reviewOpen,
    onToggleReview,
    pendingCount,
    shortcutsAnchor,
  }: {
    /** The topbar's height. */
    height?: number;
    /** The shortcuts are showing. */
    helpOpen?: boolean;
    /** The breadcrumb's way back; without it there is no breadcrumb. */
    onBack?: () => void;
    backLabel: string;
    repoName: string;
    detail: PrDetail | null;
    number: number;
    people: People;
    /** How far the PR is behind its base. */
    drift: string | null;
    actions: Snippet;
    onPrMenu: (id: string) => void;
    tab: ReviewTab;
    tabs: SegmentedOption<ReviewTab>[];
    onTabChange: (tab: ReviewTab) => void;
    filter: ReviewFilter;
    filters: SegmentedOption<ReviewFilter>[];
    onFilterChange: (filter: ReviewFilter) => void;
    changes: "all" | "since";
    onShowChanges: (changes: "all" | "since") => void;
    viewedCount: number;
    fileCount: number;
    onExpandAll: () => void;
    onCollapseAll: () => void;
    /** The Your review panel is open as an overlay. */
    reviewOpen: boolean;
    onToggleReview: () => void;
    /** Entries in the pending review. */
    pendingCount: number;
    /** Where the shortcuts open, given their button. */
    shortcutsAnchor: (button: { getBoundingClientRect(): Rect } | null) => Rect | null;
  } = $props();

  const prMenu = $derived(prMenuEntries(!!detail?.summary.isDraft));
</script>

<header bind:offsetHeight={height} class="flex shrink-0 flex-col gap-2.5 border-b border-surface-200-800 px-5 py-3">
  <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
    <!-- Where the PR is and what it is; the title truncates before the actions wrap. -->
    <div class="flex min-w-64 flex-1 items-center gap-2">
      {#if onBack}
        <button type="button" class="btn btn-sm hover:preset-tonal px-1 text-surface-500" onclick={onBack}>
          <ChevronLeft size={14} aria-hidden="true" />{backLabel}
        </button>
        <span class="text-surface-500">/</span>
        <span class="text-surface-500">{repoName}</span>
        <span class="text-surface-500">/</span>
      {/if}
      {#if detail}
        <h1 class="min-w-0 truncate text-sm leading-5 font-semibold" title={detail.summary.title}>
          {detail.summary.title}
        </h1>
        <span class="font-mono text-surface-500">#{number}</span>
        <LoginAvatar login={detail.summary.author} person={people.get(detail.summary.author)} role="Author" />
        <span class="whitespace-nowrap text-surface-500">{detail.summary.author}</span>
        {#if detail.summary.isDraft}
          <span class="status-badge preset-tonal">Draft</span>
        {/if}
        {#if drift}
          <span
            class="status-badge preset-tonal-warning whitespace-nowrap"
            title="The base has commits this PR does not"
            >{drift}</span
          >
        {/if}
      {:else}
        <span class="placeholder h-4 w-48 animate-pulse rounded"></span>
      {/if}
    </div>
    <div class="ml-auto flex shrink-0 items-center gap-2">
      {@render actions()}
      <button
        type="button"
        class="btn btn-sm hover:preset-tonal text-xs"
        disabled={!detail}
        onclick={() => detail && commands.openExternalUrl(detail.summary.url)}
      >
        Open on GitHub
      </button>
      <ActionMenu entries={prMenu} label="Pull request {number}" onSelect={onPrMenu} buttonLabel="Pull request actions">
        {#snippet row({ buttonProps })}
          <button
            {...buttonProps}
            type="button"
            class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
            disabled={!detail}
          >
            <Ellipsis size={14} aria-hidden="true" />
          </button>
        {/snippet}
      </ActionMenu>
    </div>
  </div>
  <div class="flex flex-wrap items-center gap-2">
    <Segmented label="Pull request" value={tab} options={tabs} onChange={onTabChange} />
    {#if tab === "files"}
      <Segmented label="Show comments" value={filter} options={filters} onChange={onFilterChange} />
      <Segmented
        label="Changes"
        value={changes}
        options={[
          { value: "all", label: "All changes" },
          { value: "since", label: "Since your review" },
        ]}
        onChange={onShowChanges}
      />
      <span class="grow"></span>
      <span class="text-surface-500">Viewed {viewedCount} of {fileCount}</span>
      <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" onclick={onExpandAll}>Expand all</button>
      <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" onclick={onCollapseAll}>
        Collapse all
      </button>
    {:else}
      <span class="grow"></span>
    {/if}
    <button
      type="button"
      class="btn btn-sm text-[11px] @3xl/review:hidden {reviewOpen ? "preset-tonal" : "hover:preset-tonal"}"
      aria-expanded={reviewOpen}
      onclick={onToggleReview}
    >
      <PanelRight size={14} aria-hidden="true" />Your review{pendingCount > 0 ? ` · ${pendingCount}` : ""}
    </button>
    <!-- Focus stays in the page: returning it to this button would scroll the
         reviewer back up to the topbar. -->
    <Popover
      open={helpOpen}
      onOpenChange={(details) => (helpOpen = details.open)}
      autoFocus={false}
      restoreFocus={false}
      positioning={{ placement: "bottom-end", strategy: "fixed", gutter: 6, getAnchorRect: shortcutsAnchor }}
    >
      <Popover.Trigger class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500" aria-label="Shortcuts">
        <Keyboard size={14} aria-hidden="true" />
      </Popover.Trigger>
      <Portal>
        <Popover.Positioner>
          <Popover.Content
            class="popover-card z-50 grid max-w-md grid-cols-2 gap-x-4 gap-y-1.5 p-3 text-xs"
            aria-label="Shortcuts"
          >
            {#if helpOpen}
              {#each REVIEW_SHORTCUTS as shortcut (shortcut.label)}
                <div class="flex justify-between gap-2">
                  <span>{shortcut.label}</span>
                  <span class="flex gap-1"
                    >{#each shortcut.keys as key (key)}
                      <kbd class="kbd">{key}</kbd>
                    {/each}</span
                  >
                </div>
              {/each}
            {/if}
          </Popover.Content>
        </Popover.Positioner>
      </Portal>
    </Popover>
  </div>
</header>

<!--
  The shared context menu. Right-click on the row and its ⋯ button open the
  same entries; shortcut hints show on the right, and the same shortcuts work
  on the focused row with the menu closed. Submenus open toward the side of
  the screen that has room.

  The caller renders the row and spreads `rowProps` on it and `buttonProps`
  on its ⋯ button:

    <ActionMenu entries={prMenu(pr)} label="Pull request 415" onSelect={run}>
      {#snippet row({ rowProps, buttonProps })}
        <div {...rowProps}>… <button {...buttonProps} class="row-action">…</button></div>
      {/snippet}
    </ActionMenu>

  Two Skeleton menus sit behind it, since zag anchors a menu at the pointer
  only for context triggers (the sidebar does the same).
-->
<script lang="ts">
  import { Check, ChevronRight } from "@lucide/svelte";
  import { Menu, Portal, useMenu } from "@skeletonlabs/skeleton-svelte";
  import type { Snippet } from "svelte";
  import { formatAccelerator } from "$lib/utils/accelerator";
  import { type MenuEntry, shortcutTarget } from "./actionMenu";

  type RowArgs = { rowProps: Record<string, unknown>; buttonProps: Record<string, unknown> };

  let {
    entries,
    label,
    onSelect,
    row,
    buttonLabel = "More actions",
  }: {
    entries: MenuEntry[];
    /** The menu's accessible name, e.g. "Pull request 415". */
    label: string;
    onSelect: (id: string) => void;
    row: Snippet<[RowArgs]>;
    buttonLabel?: string;
  } = $props();

  const uid = $props.id();
  const menuProps = (id: string) => ({
    id,
    "aria-label": label,
    onSelect: ({ value }: { value: string }) => onSelect(value),
    positioning: { placement: "bottom-start" as const, gutter: 4 },
  });
  const contextMenu = useMenu(menuProps(`${uid}-context`));
  const buttonMenu = useMenu(menuProps(`${uid}-button`));
  /** Submenus open to the right, or to the left when the right has no room. */
  const submenuPositioning = { placement: "right-start" as const, flip: ["left-start" as const], gutter: 2 };

  function onRowKeydown(event: KeyboardEvent) {
    // Only the row itself: keys typed into a field inside it are the field's.
    if (event.defaultPrevented || event.target !== event.currentTarget) return;
    if (contextMenu().open || buttonMenu().open) return;
    const target = shortcutTarget(entries, event);
    if (!target || target.disabled) return;
    event.preventDefault();
    onSelect(target.id);
  }

  const rowProps = $derived({
    ...contextMenu().getContextTriggerProps(),
    tabindex: 0,
    onkeydown: onRowKeydown,
  });
  const buttonProps = $derived({
    ...buttonMenu().getTriggerProps(),
    "aria-label": buttonLabel,
    title: buttonLabel,
  });
</script>

{#snippet list(items: MenuEntry[])}
  {#each items as entry, index (entry.kind === "separator" ? `separator-${index}` : entry.id)}
    {#if entry.kind === "separator"}
      <Menu.Separator class="my-1 h-px border-0 bg-surface-200-800" />
    {:else if entry.kind === "submenu"}
      <Menu aria-label={entry.label} positioning={submenuPositioning} onSelect={({ value }) => onSelect(value)}>
        <Menu.TriggerItem value={entry.id} disabled={entry.disabled} class="menu-row cursor-pointer">
          <Menu.ItemText class="grow">{entry.label}</Menu.ItemText>
          <ChevronRight size={12} class="text-surface-500" aria-hidden="true" />
        </Menu.TriggerItem>
        <Portal>
          <Menu.Positioner>
            <Menu.Content
              aria-labelledby=""
              class="popover-card z-50 flex w-[210px] min-w-0 flex-col gap-0 p-1 outline-none select-none"
            >
              {@render list(entry.entries)}
            </Menu.Content>
          </Menu.Positioner>
        </Portal>
      </Menu>
    {:else}
      <Menu.Item
        value={entry.id}
        disabled={entry.disabled}
        title={entry.title}
        class="menu-row cursor-pointer {entry.danger ? "text-error-500" : ""}"
      >
        <Menu.ItemText class="grow">{entry.label}</Menu.ItemText>
        {#if entry.hint}
          <span class="text-[10px] text-surface-500">{entry.hint}</span>
        {/if}
        {#if entry.checked}
          <Check size={12} aria-label="Current" />
        {/if}
        {#if entry.shortcut}
          <kbd class="kbd">{formatAccelerator(entry.shortcut)}</kbd>
        {/if}
      </Menu.Item>
    {/if}
  {/each}
{/snippet}

{#snippet content(menu: typeof contextMenu)}
  <Menu.Provider value={menu}>
    <Portal>
      <Menu.Positioner>
        <Menu.Content
          aria-labelledby=""
          class="popover-card z-50 flex w-[236px] min-w-0 flex-col gap-0 p-1 outline-none select-none"
        >
          {#if menu().open}
            {@render list(entries)}
          {/if}
        </Menu.Content>
      </Menu.Positioner>
    </Portal>
  </Menu.Provider>
{/snippet}

{@render row({ rowProps, buttonProps })}
{@render content(contextMenu)}
{@render content(buttonMenu)}

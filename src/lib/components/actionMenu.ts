// The entries of the shared context menu (`ActionMenu.svelte`), and the
// shortcut lookup that keeps keys live on a focused row.

import { acceleratorFromEvent } from "$lib/utils/accelerator";

/** One action. `id` is what `onSelect` receives. */
export type MenuItem = {
  kind?: "item";
  id: string;
  label: string;
  /** An accelerator (`T`, `Enter`, `CmdOrCtrl+Shift+C`, `Alt+Up`, `F2`):
   *  shown as a hint, and live on the focused row. */
  shortcut?: string;
  /** Muted text after the label, e.g. "on main". */
  hint?: string;
  /** A tick after the label: where something is now. */
  checked?: boolean;
  danger?: boolean;
  disabled?: boolean;
  /** Tooltip, for an item that needs explaining. */
  title?: string;
};

export type MenuSeparator = { kind: "separator" };

/** A nested menu. It opens toward the side of the screen that has room. */
export type MenuSubmenu = {
  kind: "submenu";
  id: string;
  label: string;
  entries: MenuEntry[];
  disabled?: boolean;
};

export type MenuEntry = MenuItem | MenuSeparator | MenuSubmenu;

function isItem(entry: MenuEntry): entry is MenuItem {
  return entry.kind === undefined || entry.kind === "item";
}

/** The enabled top-level item whose shortcut `event` presses, if any.
 *  Submenu items are reached through the menu, not by shortcut. */
export function shortcutTarget(entries: MenuEntry[], event: KeyboardEvent): MenuItem | null {
  const pressed = acceleratorFromEvent(event);
  if (!pressed) return null;
  return entries.find((entry): entry is MenuItem => isItem(entry) && entry.shortcut === pressed) ?? null;
}

/** What `ActionMenu` gives its row: spread `rowProps` on the row and
 *  `buttonProps` on its ⋯ button. */
export type RowArgs = { rowProps: Record<string, unknown>; buttonProps: Record<string, unknown> };

// The entries of the review page's PR menu (⋯), and what a chosen id means.
// What an entry does needs the page, so that stays there.

import type { MenuEntry } from "$lib/components/actionMenu";
import type { MergeMethod } from "$lib/types";

export function prMenuEntries(isDraft: boolean): MenuEntry[] {
  return [
    {
      kind: "submenu",
      id: "merge",
      label: "Merge",
      entries: [
        { id: "merge:merge", label: "Create a merge commit" },
        { id: "merge:squash", label: "Squash and merge" },
        { id: "merge:rebase", label: "Rebase and merge" },
      ],
    },
    ...(isDraft ? [{ id: "ready", label: "Mark ready for review" }] : []),
    { id: "retarget", label: "Retarget…" },
    { kind: "separator" },
    { id: "close", label: "Close pull request…", danger: true },
  ];
}

/** The merge method a menu id asks for, when it is one of Merge's entries. */
export function mergeMethod(id: string): MergeMethod | null {
  return id.startsWith("merge:") ? (id.slice("merge:".length) as MergeMethod) : null;
}

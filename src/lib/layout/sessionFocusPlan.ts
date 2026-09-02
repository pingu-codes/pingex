/**
 * What the sidebar's session-focus button does to the current data: hide
 * every thread neither opened this session nor favorited (the open one
 * stays), then collapse projects and folders left without a visible thread.
 * Pure, so the three lists can be checked without the backend.
 */
import { isTouched } from "$lib/layout/sessionFocus.svelte";
import { buildTree, flattenItems, ROOT_SCOPE, type TreeNode } from "$lib/layout/sidebarTree";
import type { Project, SidebarLayout, ThreadSummary } from "$lib/types";

export interface SessionFocusPlan {
  hide: string[];
  collapseProjects: string[];
  collapseFolders: string[];
}

const threadAdapter = { key: (thread: ThreadSummary) => thread.id, pinned: (thread: ThreadSummary) => thread.pinned };
const projectAdapter = { key: (project: Project) => project.path, pinned: (project: Project) => project.pinned };

export function sessionFocusPlan(
  projects: Project[],
  layout: SidebarLayout,
  selectedThread: string | null,
): SessionFocusPlan {
  const plan: SessionFocusPlan = { hide: [], collapseProjects: [], collapseFolders: [] };
  const keep = (thread: ThreadSummary) => thread.id === selectedThread || thread.pinned || isTouched(thread.id);
  const collapseEmptyFolders = <T>(nodes: TreeNode<T>[]) => {
    for (const node of nodes) {
      if (node.kind !== "folder") continue;
      if (flattenItems(node.children).length === 0) {
        if (node.folder.expanded) plan.collapseFolders.push(node.id);
      } else collapseEmptyFolders(node.children);
    }
  };
  const live: Project[] = [];
  for (const project of projects) {
    if (project.archived) continue;
    const remaining = project.threads.filter(keep);
    plan.hide.push(...project.threads.filter((thread) => !keep(thread) && !thread.hidden).map((thread) => thread.id));
    if (remaining.length === 0) {
      if (project.expanded) plan.collapseProjects.push(project.path);
    } else {
      live.push(project);
      collapseEmptyFolders(buildTree(layout, project.path, remaining, threadAdapter));
    }
  }
  collapseEmptyFolders(buildTree(layout, ROOT_SCOPE, live, projectAdapter));
  return plan;
}

export const isEmptyPlan = (plan: SessionFocusPlan) =>
  plan.hide.length === 0 && plan.collapseProjects.length === 0 && plan.collapseFolders.length === 0;

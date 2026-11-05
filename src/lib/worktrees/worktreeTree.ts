import type { Project, ThreadSummary, WorktreeEntry } from "$lib/types";
import { basename, trimSlash } from "$lib/utils/path";
import { branchLabel } from "$lib/worktrees/worktrees";

/** A worktree drawn under its repository's row. */
export interface WorktreeChild {
  path: string;
  name: string;
  branch: string;
  /** The sidebar project for this worktree, or null when it was never added. */
  adopted: Project | null;
  /** For an unadded worktree: its threads, filed by cwd. */
  threads: ThreadSummary[];
}

export interface NestedProjects {
  /** Top-level rows: every project except the worktrees nested under a repo. */
  roots: Project[];
  /** Worktree children per repository path. */
  children: Record<string, WorktreeChild[]>;
  /** Unadded worktree path → the repository project it belongs to. */
  unaddedParents: Record<string, string>;
}

/** Linked worktrees worth a row: on disk, not the main tree, not scaffolding. */
function linked(entries: WorktreeEntry[], repoPath: string): WorktreeEntry[] {
  return entries.filter(
    (entry) =>
      !entry.isMain &&
      !entry.bare &&
      !entry.missingDir &&
      !entry.prunable &&
      !entry.isTemp &&
      trimSlash(entry.path) !== trimSlash(repoPath),
  );
}

const within = (cwd: string, root: string) => cwd === root || cwd.startsWith(`${root}/`);

/**
 * Regroup the sidebar's projects so each repository carries its worktrees as
 * children. An adopted worktree project moves under its repository; a
 * worktree nobody added becomes a child row holding the threads whose cwd is
 * inside it, taken from the unfiled threads and from the repository itself
 * (a worktree inside the repo, like `.claude/worktrees/x`, would otherwise
 * lend its threads to the repo). Pure, so the sidebar can derive it.
 */
export function nestProjects(
  projects: Project[],
  worktreesByRepo: Record<string, WorktreeEntry[] | undefined>,
  unfiled: ThreadSummary[],
): NestedProjects {
  const byPath = new Map(projects.map((project) => [trimSlash(project.path), project]));
  const children: Record<string, WorktreeChild[]> = {};
  const unaddedParents: Record<string, string> = {};
  const nestedAdopted = new Set<string>();
  /** Unadded worktree roots, longest first so the deepest match wins. */
  const unadded: { root: string; child: WorktreeChild }[] = [];

  for (const repo of projects) {
    if (repo.kind !== "folder") continue;
    const entries = worktreesByRepo[repo.path];
    if (!entries) continue;
    for (const entry of linked(entries, repo.path)) {
      const root = trimSlash(entry.path);
      if (nestedAdopted.has(root) || root in unaddedParents) continue;
      const adopted = byPath.get(root) ?? null;
      // A worktree can't parent its own repository's row.
      if (adopted && adopted.kind !== "worktree") continue;
      const child: WorktreeChild = {
        path: entry.path,
        name: adopted?.name ?? basename(entry.path),
        branch: branchLabel(entry),
        adopted,
        threads: [],
      };
      (children[repo.path] ??= []).push(child);
      if (adopted) nestedAdopted.add(root);
      else {
        unaddedParents[root] = repo.path;
        unadded.push({ root, child });
      }
    }
  }
  unadded.sort((a, b) => b.root.length - a.root.length);

  const claim = (thread: ThreadSummary): WorktreeChild | null => {
    const cwd = trimSlash(thread.cwd);
    return unadded.find(({ root }) => within(cwd, root))?.child ?? null;
  };
  for (const thread of unfiled) claim(thread)?.threads.push(thread);

  const roots: Project[] = [];
  for (const project of projects) {
    if (nestedAdopted.has(trimSlash(project.path))) continue;
    if (project.kind !== "folder" || !children[project.path]?.some((child) => !child.adopted)) {
      roots.push(project);
      continue;
    }
    const kept: ThreadSummary[] = [];
    for (const thread of project.threads) {
      const child = claim(thread);
      if (child) child.threads.push(thread);
      else kept.push(thread);
    }
    roots.push(kept.length === project.threads.length ? project : { ...project, threads: kept });
  }
  for (const list of Object.values(children)) {
    for (const child of list)
      child.threads.sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.updatedAt - a.updatedAt);
  }
  return { roots, children, unaddedParents };
}

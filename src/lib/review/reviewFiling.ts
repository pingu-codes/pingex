// Where the sidebar files a thread that has a PR link. Derived the way
// worktree nesting is: pure over the projects, the unfiled threads and the
// links, so the Sidebar only draws the result.
//
// A thread on someone else's PR, or on my PR that has no worktree, goes in
// its repository's Reviews group. A thread on my PR in a worktree keeps the
// cwd nesting and shows a PR chip. Once the PR closes the thread goes back
// to where its cwd files it.

import type { PrLink, Project, ThreadSummary } from "$lib/types";

export interface ReviewThread {
  thread: ThreadSummary;
  link: PrLink;
}

export interface ReviewFiling {
  /** Reviews group rows by repository path, most recent first. */
  reviews: Record<string, ReviewThread[]>;
  /** Threads a Reviews group took; project lists and nesting leave them out. */
  taken: Set<string>;
}

/** Whether a link files its thread under Reviews rather than by cwd. */
export function filedUnderReviews(link: PrLink): boolean {
  return link.closedAt === null && (!link.mine || link.step !== "worktree");
}

export function fileReviewThreads(
  projects: Project[],
  unfiled: ThreadSummary[],
  links: Record<string, PrLink>,
): ReviewFiling {
  const repositories = new Set(projects.map((project) => project.path));
  const reviews: Record<string, ReviewThread[]> = {};
  const taken = new Set<string>();
  const all = [...projects.flatMap((project) => project.threads), ...unfiled];
  for (const thread of all) {
    const link = links[thread.id];
    if (!link || taken.has(thread.id) || !filedUnderReviews(link) || !repositories.has(link.repoKey)) continue;
    taken.add(thread.id);
    (reviews[link.repoKey] ??= []).push({ thread, link });
  }
  for (const rows of Object.values(reviews)) rows.sort((a, b) => b.thread.updatedAt - a.thread.updatedAt);
  return { reviews, taken };
}

/** A project with the threads a Reviews group took left out. */
export function withoutTaken(project: Project, taken: Set<string>): Project {
  if (!project.threads.some((thread) => taken.has(thread.id))) return project;
  return { ...project, threads: project.threads.filter((thread) => !taken.has(thread.id)) };
}

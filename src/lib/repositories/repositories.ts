// Pure rules for Add repository: which GitHub repositories to offer and in
// what order, reading a repository URL, and the PR source choices a folder's
// remotes allow.

import type { GitHubRepo, GitRemote, KnownClone, PrSource, ReviewRequests } from "$lib/types";

/** How a GitHub repository gets added. */
export type AddMode = "prsOnly" | "clone" | "link";

export type RepositoryChoice = {
  /** `owner/name`, lowercase. */
  slug: string;
  /** Open review requests for me. */
  requests: number;
  /** Already feeding the inbox: shown, not offered. */
  added: boolean;
  /** A clone Pingex knows, linked instead of cloned. */
  clone: KnownClone | null;
  fork: string | null;
};

/** Review requests not yet added come first, most requests first; then my
 *  repositories, each listed once. `query` filters both by slug. */
export function githubChoices(
  mine: readonly GitHubRepo[],
  requests: readonly ReviewRequests[],
  added: ReadonlySet<string>,
  clones: readonly KnownClone[],
  query = "",
): { asking: RepositoryChoice[]; yours: RepositoryChoice[] } {
  const needle = query.trim().toLowerCase();
  const matches = (slug: string) => !needle || slug.includes(needle);
  const cloneOf = (slug: string) => clones.find((clone) => clone.slug.toLowerCase() === slug) ?? null;
  const asking = requests
    .map((request) => ({ ...request, slug: request.slug.toLowerCase() }))
    .filter((request) => request.count > 0 && !added.has(request.slug))
    .sort((a, b) => b.count - a.count || a.slug.localeCompare(b.slug))
    .map((request) => ({
      slug: request.slug,
      requests: request.count,
      added: false,
      clone: cloneOf(request.slug),
      fork: null,
    }));
  const askingSlugs = new Set(asking.map((choice) => choice.slug));
  const yours = mine
    .map((repo) => ({ repo, slug: repo.slug.toLowerCase() }))
    .filter(({ slug }) => !askingSlugs.has(slug))
    .map(({ repo, slug }) => ({
      slug,
      requests: requests.find((request) => request.slug.toLowerCase() === slug)?.count ?? 0,
      added: added.has(slug),
      clone: cloneOf(slug),
      fork: repo.parent,
    }));
  return { asking: asking.filter((c) => matches(c.slug)), yours: yours.filter((c) => matches(c.slug)) };
}

/** The mode a choice starts in: linked when a clone exists, else PRs only. */
export function defaultMode(choice: RepositoryChoice): AddMode {
  return choice.clone ? "link" : "prsOnly";
}

const SLUG = /^([A-Za-z0-9][A-Za-z0-9-]*)\/([A-Za-z0-9._-]+?)(?:\.git)?\/?$/;

/** A GitHub repository from what the user typed: `owner/name`, an https
 *  URL, or an SSH URL. `null` for anything else. */
export function parseRepositoryInput(text: string): { slug: string; cloneUrl: string; name: string } | null {
  const trimmed = text.trim();
  const path =
    /^https?:\/\/(?:www\.)?github\.com\/(.+)$/i.exec(trimmed)?.[1] ??
    /^(?:ssh:\/\/)?git@github\.com[:/](.+)$/i.exec(trimmed)?.[1] ??
    trimmed;
  const match = SLUG.exec(path);
  if (!match || match[2].startsWith(".")) return null;
  const slug = `${match[1]}/${match[2]}`.toLowerCase();
  const isSsh = /^(?:ssh:\/\/)?git@/i.test(trimmed);
  return {
    slug,
    cloneUrl: isSsh ? `git@github.com:${slug}.git` : `https://github.com/${slug}.git`,
    name: match[2],
  };
}

/** The folder a clone of `slug` takes. */
export function cloneFolderName(slug: string): string {
  return slug.split("/").at(-1) ?? slug;
}

export type SourceOption = {
  value: PrSource;
  /** The remote's name, or Both / None. */
  label: string;
  slug: string | null;
  description: string | null;
};

/** The PR sources a folder's remotes allow: each GitHub `origin` or
 *  `upstream`, Both when there are two, and None. */
export function sourceOptions(remotes: readonly GitRemote[]): SourceOption[] {
  const github = (name: "origin" | "upstream") => remotes.find((remote) => remote.name === name && remote.slug);
  const upstream = github("upstream");
  const origin = github("origin");
  const options: SourceOption[] = [];
  if (upstream)
    options.push({
      value: "upstream",
      label: "upstream",
      slug: upstream.slug,
      description: "Where PRs from this fork go. Only ones that ask for you or that you wrote.",
    });
  if (origin) options.push({ value: "origin", label: "origin", slug: origin.slug, description: null });
  if (upstream && origin) options.push({ value: "both", label: "Both", slug: null, description: null });
  options.push({ value: "none", label: "None", slug: null, description: "Threads and worktrees only, no inbox" });
  return options;
}

// Add repository in the browser preview and the repository UI tests: the
// GitHub account, its repositories and review requests, folders on disk, and
// the PRs-only list and settings the dialog writes, all on the preview desk.
// A desk repository's clone is a folder it knows, so every desk screen agrees
// on what is added.

import type { FolderFacts, PrSource, Repository, RepositorySettings } from "$lib/types";
import type { DeskCommands } from "./commands";
import { defaultSettings, desk, deskInbox } from "./desk";

const remote = (name: string, slug: string | null) => ({
  name,
  url: slug ? `https://github.com/${slug}.git` : "/srv/git/notes.git",
  slug,
});

function folderFacts(path: string): Omit<FolderFacts, "prSource"> {
  const trimmed = path.replace(/\/+$/, "");
  const known = desk().folders.get(trimmed);
  if (known) return structuredClone(known);
  const repository = desk().repositories.find((repo) => repo.path === trimmed);
  if (repository?.path)
    return {
      root: repository.path,
      name: repository.name,
      branch: "main",
      linkedWorktrees: 0,
      remotes: [remote("origin", repository.slug || null)],
      lockfile: null,
      setupCommand: null,
    };
  const cloned = desk().clones.get(trimmed);
  if (cloned)
    return {
      root: trimmed,
      name: trimmed.split("/").at(-1) ?? trimmed,
      branch: "main",
      linkedWorktrees: 0,
      remotes: [remote("origin", cloned)],
      lockfile: null,
      setupCommand: null,
    };
  throw new Error("This folder is not a Git repository");
}

function defaultSource(facts: Omit<FolderFacts, "prSource">): PrSource {
  const github = (name: string) => facts.remotes.some((remote) => remote.name === name && remote.slug);
  return github("upstream") ? "upstream" : github("origin") ? "origin" : "none";
}

function storedSettings(key: string, source: PrSource): RepositorySettings {
  return desk().settings.get(key) ?? defaultSettings(key, source);
}

function repositoryAt(path: string): Repository {
  let facts: Omit<FolderFacts, "prSource">;
  try {
    facts = folderFacts(path);
  } catch {
    return { key: path, name: path.split("/").at(-1) ?? path, slug: "", path, state: "noRemote", feeds: [] };
  }
  const source = storedSettings(facts.root, defaultSource(facts)).prSource;
  const names = { origin: ["origin"], upstream: ["upstream"], both: ["origin", "upstream"], none: [] }[source];
  const feeds = names.flatMap((name) => {
    const found = facts.remotes.find((remote) => remote.name === name && remote.slug);
    return found?.slug ? [{ remote: name, slug: found.slug, onlyMine: name === "upstream" }] : [];
  });
  return {
    key: facts.root,
    name: facts.name,
    slug: feeds[0]?.slug ?? "",
    path: facts.root,
    state: feeds.length ? "cloned" : "noRemote",
    feeds,
  };
}

export const repositoryCommands = {
  reviewMyRepositories: () => structuredClone(desk().githubRepositories),
  reviewRepositoriesRequestingReview: () =>
    [
      ...desk()
        .repositories.filter((repo) => repo.slug)
        .map((repo) => ({
          slug: repo.slug,
          count: deskInbox(repo.path ?? repo.slug).prs.filter((pr) => pr.reviewRequests.includes(desk().viewer)).length,
        })),
      ...structuredClone(desk().requests),
    ]
      .filter((repo) => repo.count > 0)
      .sort((a, b) => b.count - a.count),
  reviewGithubAccount: () => structuredClone(desk().account),
  reviewRepositoryAccount(repoDir) {
    const { account } = desk();
    const pin = desk().accountPins.get(repoDir) ?? null;
    const missing = pin !== null && !account.logins.includes(pin);
    return {
      login: pin ?? account.login,
      source: missing ? "pinMissing" : pin ? "pinned" : "global",
      pin,
      logins: [...account.logins],
      global: account.login,
    };
  },
  reviewSetRepositoryAccount(repoDir, login) {
    const { account, accountPins } = desk();
    if (login === null) accountPins.delete(repoDir);
    else if (account.logins.includes(login)) accountPins.set(repoDir, login);
    else throw new Error(`${login} is not signed in to gh`);
    return null;
  },
  reviewSwitchGithubAccount(login) {
    const state = desk();
    if (!state.account.logins.includes(login)) throw new Error("That account is not signed in");
    state.account = { ...state.account, login };
    return null;
  },
  reviewInspectFolder(folder) {
    const facts = folderFacts(folder);
    return { ...facts, prSource: storedSettings(facts.root, defaultSource(facts)).prSource };
  },
  reviewRepository: (repoDir) => repositoryAt(repoDir),
  reviewRepositorySettings(repoDir) {
    try {
      return structuredClone(storedSettings(repoDir, defaultSource(folderFacts(repoDir))));
    } catch {
      return structuredClone(storedSettings(repoDir, "none"));
    }
  },
  reviewSaveRepositorySettings(repoDir, next) {
    desk().settings.set(repoDir, { ...next, repoKey: repoDir, updatedAt: Math.floor(Date.now() / 1000) });
    return null;
  },
  reviewLinkClone(repoDir, slug) {
    const facts = folderFacts(repoDir);
    const found = facts.remotes.find((remote) => remote.slug === slug.toLowerCase());
    if (!found) throw new Error(`This folder has no remote for ${slug}`);
    const current = storedSettings(facts.root, defaultSource(facts));
    const widened: PrSource =
      current.prSource === "none" || current.prSource === found.name ? (found.name as PrSource) : "both";
    desk().settings.set(facts.root, { ...current, prSource: widened, updatedAt: 1 });
    return repositoryAt(facts.root);
  },
  reviewFindClones: (slugs) => structuredClone(desk().knownClones.filter((clone) => slugs.includes(clone.slug))),
  reviewCloneRepository(parentDir, url, name) {
    const path = `${parentDir.replace(/^~/, "/Users/ciaran").replace(/\/+$/, "")}/${name}`;
    const slug = /github\.com[/:]([^/]+\/[^/.]+)/.exec(url)?.[1]?.toLowerCase();
    desk().clones.set(path, slug ?? "");
    return path;
  },
  reviewPrsOnlyRepositories: () =>
    desk().prsOnly.map((slug) => ({
      key: slug,
      name: slug.split("/")[1] ?? slug,
      slug,
      path: null,
      state: "prsOnly" as const,
      feeds: [{ remote: "", slug, onlyMine: false }],
    })),
  reviewAddPrsOnlyRepository(slug) {
    const state = desk();
    if (!state.prsOnly.includes(slug.toLowerCase())) state.prsOnly = [...state.prsOnly, slug.toLowerCase()];
    return null;
  },
  reviewRemovePrsOnlyRepository(slug) {
    const state = desk();
    state.prsOnly = state.prsOnly.filter((candidate) => candidate !== slug.toLowerCase());
    return null;
  },
} satisfies Partial<DeskCommands>;

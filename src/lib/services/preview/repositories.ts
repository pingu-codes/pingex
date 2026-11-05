// Add repository in the browser preview and the repository UI tests: the
// GitHub account, its repositories and review requests, folders on disk, and
// the PRs-only list and settings the dialog writes. Built on the desk's
// repositories (`DESK_REPOS`), so every desk screen agrees on what is added.

import type {
  FolderFacts,
  GitHubAccount,
  GitHubRepo,
  KnownClone,
  PrSource,
  Repository,
  RepositorySettings,
  ReviewRequests,
} from "$lib/types";
import type { PreviewCommands } from "./commands";
import { DESK_REPOS, DESK_VIEWER, deskInbox } from "./desk";

/** A GitHub repository the account owns. */
function mine(name: string, fork: string | null = null): GitHubRepo {
  const slug = `${DESK_VIEWER}/${name}`;
  return {
    slug,
    private: false,
    fork: fork !== null,
    parent: fork,
    defaultBranch: "main",
    cloneUrl: `https://github.com/${slug}.git`,
    sshUrl: `git@github.com:${slug}.git`,
  };
}

export const PREVIEW_MY_REPOS: GitHubRepo[] = [
  mine("codex-custom"),
  mine("arctic-explorer"),
  mine("website"),
  mine("dotfiles"),
  mine("codex-mirror", "openai/codex"),
];

/** Not added yet: one with no clone anywhere, one with a clone Pingex knows. */
const EXTRA_REQUESTS: ReviewRequests[] = [
  { slug: "tomasz/bun-tools", count: 2 },
  { slug: "ciaran/website", count: 1 },
];

/** Folders Pingex knows that clone a repository nobody added. */
const KNOWN_CLONES: KnownClone[] = [
  { slug: "ciaran/website", path: "/Users/ciaran/Projects/website", remote: "origin" },
];

/** A fork on disk: `origin` is mine, `upstream` is where its PRs open. */
export const FORK_FOLDER = "/Users/ciaran/Projects/codex-mirror";

const remote = (name: string, slug: string | null) => ({
  name,
  url: slug ? `https://github.com/${slug}.git` : "/srv/git/notes.git",
  slug,
});

const FOLDERS: Record<string, Omit<FolderFacts, "prSource">> = {
  [FORK_FOLDER]: {
    root: FORK_FOLDER,
    name: "codex-mirror",
    branch: "main",
    linkedWorktrees: 2,
    remotes: [remote("origin", "ciaran/codex-mirror"), remote("upstream", "openai/codex")],
    lockfile: "bun.lock",
    setupCommand: "bun install",
  },
  "/Users/ciaran/Projects/website": {
    root: "/Users/ciaran/Projects/website",
    name: "website",
    branch: "main",
    linkedWorktrees: 0,
    remotes: [remote("origin", "ciaran/website")],
    lockfile: null,
    setupCommand: null,
  },
  "/Users/ciaran/Projects/notes": {
    root: "/Users/ciaran/Projects/notes",
    name: "notes",
    branch: "main",
    linkedWorktrees: 0,
    remotes: [remote("origin", null)],
    lockfile: null,
    setupCommand: null,
  },
};

const ACCOUNT: GitHubAccount = { login: DESK_VIEWER, tokenSource: "keyring", logins: [DESK_VIEWER, "ciaran-work"] };

let account = structuredClone(ACCOUNT);
let prsOnly: string[] = [];
const settings = new Map<string, RepositorySettings>();
const clones = new Map<string, string>();

/** Sign the preview out of GitHub, or in as someone else. */
export function setPreviewGithubAccount(next: GitHubAccount) {
  account = structuredClone(next);
}

/** Back to the base data; tests call it between cases. */
export function resetPreviewRepositories() {
  account = structuredClone(ACCOUNT);
  prsOnly = DESK_REPOS.filter((repo) => repo.state === "prsOnly").map((repo) => repo.slug);
  settings.clear();
  clones.clear();
}
resetPreviewRepositories();

function folderFacts(path: string): Omit<FolderFacts, "prSource"> {
  const trimmed = path.replace(/\/+$/, "");
  const known = FOLDERS[trimmed];
  if (known) return structuredClone(known);
  const desk = DESK_REPOS.find((repo) => repo.path === trimmed);
  if (desk?.path)
    return {
      root: desk.path,
      name: desk.name,
      branch: "main",
      linkedWorktrees: 0,
      remotes: [remote("origin", desk.slug || null)],
      lockfile: null,
      setupCommand: null,
    };
  const cloned = clones.get(trimmed);
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
  return (
    settings.get(key) ?? {
      repoKey: key,
      setupCommand: null,
      reviewCheckoutPolicy: "ask",
      skipCheckoutConfirm: false,
      worktreeDir: null,
      prSource: source,
      updatedAt: 0,
    }
  );
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

export const repositoryCommands: PreviewCommands = {
  reviewMyRepositories: () => structuredClone(PREVIEW_MY_REPOS),
  reviewRepositoriesRequestingReview: () =>
    [
      ...DESK_REPOS.filter((repo) => repo.slug).map((repo) => ({
        slug: repo.slug,
        count: deskInbox(repo.path ?? repo.slug).prs.filter((pr) => pr.reviewRequests.includes(DESK_VIEWER)).length,
      })),
      ...EXTRA_REQUESTS,
    ]
      .filter((repo) => repo.count > 0)
      .sort((a, b) => b.count - a.count),
  reviewGithubAccount: () => structuredClone(account),
  reviewSwitchGithubAccount(login) {
    if (!account.logins.includes(login)) throw new Error("That account is not signed in");
    account = { ...account, login };
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
    settings.set(repoDir, { ...next, repoKey: repoDir, updatedAt: Math.floor(Date.now() / 1000) });
  },
  reviewLinkClone(repoDir, slug) {
    const facts = folderFacts(repoDir);
    const found = facts.remotes.find((remote) => remote.slug === slug.toLowerCase());
    if (!found) throw new Error(`This folder has no remote for ${slug}`);
    const current = storedSettings(facts.root, defaultSource(facts));
    const widened: PrSource =
      current.prSource === "none" || current.prSource === found.name ? (found.name as PrSource) : "both";
    settings.set(facts.root, { ...current, prSource: widened, updatedAt: 1 });
    return repositoryAt(facts.root);
  },
  reviewFindClones: (slugs) => structuredClone(KNOWN_CLONES.filter((clone) => slugs.includes(clone.slug))),
  reviewCloneRepository(parentDir, url, name) {
    const path = `${parentDir.replace(/^~/, "/Users/ciaran").replace(/\/+$/, "")}/${name}`;
    const slug = /github\.com[/:]([^/]+\/[^/.]+)/.exec(url)?.[1]?.toLowerCase();
    clones.set(path, slug ?? "");
    return path;
  },
  reviewPrsOnlyRepositories: () =>
    prsOnly.map((slug) => ({
      key: slug,
      name: slug.split("/")[1] ?? slug,
      slug,
      path: null,
      state: "prsOnly" as const,
      feeds: [{ remote: "", slug, onlyMine: false }],
    })),
  reviewAddPrsOnlyRepository(slug) {
    if (!prsOnly.includes(slug.toLowerCase())) prsOnly = [...prsOnly, slug.toLowerCase()];
  },
  reviewRemovePrsOnlyRepository(slug) {
    prsOnly = prsOnly.filter((candidate) => candidate !== slug.toLowerCase());
  },
};

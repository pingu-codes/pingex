// One cache for the git and GitHub reads the UI repeats on every navigation:
// a PR's detail, a stack's overview, a thread's changes, a repository's
// worktrees. A screen that opens again shows the last answer at once and
// refreshes it behind the scenes (stale-while-revalidate), so leaving a page
// no longer means waiting for git and `gh` again on the way back.
//
// Every read has a policy in `READS`: under `fresh` it is served as is;
// between `fresh` and `expire` it is served and re-read in the background;
// past `expire` it is dropped and read again with the placeholder showing.
// Nothing outlives `expire`, and the cache holds at most `MAX_ENTRIES`.
//
// Staleness also comes from events, not only time:
// - every git or review mutation through the `commands` proxy marks what it
//   could have changed (`invalidateAfter`, below);
// - window focus and a finished agent turn mark local git state stale
//   (`app/listeners.svelte.ts`).
// Marked entries that a mounted component is showing re-read at once; the
// rest re-read when next asked for.

import { untrack } from "svelte";
import { argNamed, onCommandSettled, type SettledCommand } from "$lib/services/commandEvents";
import { commands } from "$lib/services/homeRouting";
import { errorMessage } from "$lib/utils/errors";

type Commands = typeof commands;

/** What a read depends on, which decides what makes it stale.
 *  - `local`: the working tree and refs; stale after any git mutation, on
 *    focus and when an agent turn finishes.
 *  - `github`: GitHub (and the clone's view of it); stale after a review
 *    mutation on the same repository.
 *  - `provider`: whether `gh` is installed and signed in.
 *  - `sha`: keyed by commit SHAs, so it never changes; only time evicts it. */
export type ReadKind = "local" | "github" | "provider" | "sha";

type Policy = {
  /** Served without re-reading while younger than this. */
  fresh: number;
  /** Dropped once older than this. */
  expire: number;
  kind: ReadKind;
  /** The index of the repository argument, for repository-scoped invalidation. */
  repoArg?: number;
};

const SECOND = 1000;
const MINUTE = 60 * SECOND;

/** Local reads whose first argument is the directory, so a path-scoped
 *  refresh can name it. */
const LOCAL = { fresh: 2 * SECOND, expire: 10 * MINUTE, kind: "local", repoArg: 0 } as const;
const GITHUB = { fresh: 30 * SECOND, expire: 10 * MINUTE, kind: "github", repoArg: 0 } as const;
const BY_SHA = { fresh: 30 * MINUTE, expire: 30 * MINUTE, kind: "sha", repoArg: 0 } as const;

/** Every read that goes through the cache. A command missing here is read
 *  directly, every time. */
export const READS = {
  gitStatus: LOCAL,
  gitChangesSummary: LOCAL,
  gitWorktrees: LOCAL,
  gitRepoInfo: LOCAL,
  gitContext: LOCAL,
  gitBranches: LOCAL,
  gitRecentCommits: LOCAL,
  gitFileDiff: LOCAL,
  gitStagedFileDiff: LOCAL,
  // Keyed by thread id, not a directory.
  reviewBranchProgress: { fresh: 2 * SECOND, expire: 10 * MINUTE, kind: "local" },
  reviewCheckoutState: LOCAL,
  reviewPrDetail: GITHUB,
  reviewListPrs: GITHUB,
  reviewStackOverview: GITHUB,
  reviewRestackPlan: GITHUB,
  reviewPrsBehind: GITHUB,
  reviewProviderStatus: { fresh: 5 * MINUTE, expire: 60 * MINUTE, kind: "provider", repoArg: 0 },
  reviewDiff: BY_SHA,
  reviewFile: BY_SHA,
  reviewChangesSince: BY_SHA,
} as const satisfies Partial<Record<keyof Commands, Policy>>;

export type ReadName = keyof typeof READS;
/** A read's arguments. Callers leave out the trailing `window`, which the
 *  proxy fills in. */
export type ReadArgs<N extends ReadName> = Parameters<Commands[N]>;
export type ReadResult<N extends ReadName> = Awaited<ReturnType<Commands[N]>>;

/** At most this many entries; the least recently used unobserved ones go first. */
const MAX_ENTRIES = 300;

class Entry {
  value = $state.raw<unknown>(undefined);
  error = $state<string | null>(null);
  loading = $state(false);
  /** When the value landed; 0 while there is none. */
  fetchedAt = 0;
  /** Marked by an event; the next read waits for a new answer. */
  stale = false;
  /** A mutation landed while a read was in flight, so that read may predate
   *  it: read again once it settles. */
  rerun = false;
  inflight: Promise<void> | null = null;
  observers = 0;
  usedAt = Date.now();

  constructor(
    readonly key: string,
    readonly name: ReadName,
    readonly args: readonly unknown[],
    readonly policy: Policy,
  ) {}

  get repo(): unknown {
    return this.policy.repoArg === undefined ? undefined : this.args[this.policy.repoArg];
  }

  /** Whether there is a value young enough to show. */
  has(now = Date.now()): boolean {
    return this.fetchedAt > 0 && now - this.fetchedAt < this.policy.expire;
  }

  isFresh(now = Date.now()): boolean {
    return this.has(now) && !this.stale && now - this.fetchedAt < this.policy.fresh;
  }
}

const entries = new Map<string, Entry>();

function keyOf(name: string, args: readonly unknown[]): string {
  const trimmed = [...args];
  while (trimmed.length > 0 && trimmed[trimmed.length - 1] == null) trimmed.pop();
  return JSON.stringify([name, ...trimmed]);
}

function entryFor(name: ReadName, args: readonly unknown[]): Entry {
  const key = keyOf(name, args);
  let entry = entries.get(key);
  if (!entry) {
    sweep();
    entry = new Entry(key, name, args, READS[name]);
    entries.set(key, entry);
  }
  entry.usedAt = Date.now();
  return entry;
}

/** Drop expired entries nobody shows, then the least recently used ones
 *  beyond the cap. */
function sweep(now = Date.now()): void {
  for (const [key, entry] of entries) {
    if (entry.observers === 0 && !entry.inflight && !entry.has(now)) entries.delete(key);
  }
  if (entries.size < MAX_ENTRIES) return;
  const idle = [...entries.values()]
    .filter((entry) => entry.observers === 0 && !entry.inflight)
    .sort((a, b) => a.usedAt - b.usedAt);
  for (const entry of idle.slice(0, entries.size - MAX_ENTRIES + 1)) entries.delete(entry.key);
}

/** Read `entry` from the backend, or join the read already in flight. */
function load(entry: Entry): Promise<void> {
  if (entry.inflight) return entry.inflight;
  entry.loading = true;
  const method = commands[entry.name] as (...args: unknown[]) => Promise<unknown>;
  const read = untrack(() => method(...entry.args)).then(
    (value) => {
      entry.value = value;
      entry.error = null;
      entry.fetchedAt = Date.now();
      entry.stale = false;
    },
    (cause) => {
      entry.error = errorMessage(cause);
    },
  );
  entry.inflight = read.finally(() => {
    entry.inflight = null;
    entry.loading = false;
    if (entry.rerun) {
      entry.rerun = false;
      entry.stale = true;
      if (entry.observers > 0) void load(entry);
    }
  });
  return entry.inflight;
}

/** Read again when the value is missing, aged past `fresh` or marked stale.
 *  An expired value is cleared first, so it is never shown. */
function revalidate(entry: Entry): Promise<void> {
  const now = Date.now();
  if (entry.isFresh(now)) return Promise.resolve();
  if (entry.fetchedAt > 0 && !entry.has(now)) {
    entry.value = undefined;
    entry.fetchedAt = 0;
  }
  return load(entry);
}

function settledValue<N extends ReadName>(entry: Entry): ReadResult<N> {
  if (entry.has()) return entry.value as ReadResult<N>;
  throw new Error(entry.error ?? "No answer");
}

/**
 * The read's answer, for code outside a component (or a one-off read inside
 * one). A value aged past `fresh` is returned at once and re-read in the
 * background; a missing, expired or event-marked one is waited for.
 */
export async function fetchQuery<N extends ReadName>(name: N, ...args: ReadArgs<N>): Promise<ReadResult<N>> {
  const entry = entryFor(name, args);
  const now = Date.now();
  if (entry.isFresh(now)) return entry.value as ReadResult<N>;
  if (entry.has(now) && !entry.stale) {
    void load(entry);
    return entry.value as ReadResult<N>;
  }
  await revalidate(entry);
  // The read joined may have started before the event that marked the
  // entry; it is then marked again, and one more read is the answer.
  if (entry.stale) await revalidate(entry);
  return settledValue<N>(entry);
}

/** The cached answer if one is young enough to show, without reading. For
 *  a page that keeps its own state: show this at once, then `readFresh`. */
export function peekQuery<N extends ReadName>(name: N, ...args: ReadArgs<N>): ReadResult<N> | undefined {
  const entry = entries.get(keyOf(name, args));
  return entry?.has() ? (entry.value as ReadResult<N>) : undefined;
}

/** A current answer: the cached one only while it is fresh and unmarked,
 *  otherwise a new read (joining one in flight) that this waits for. */
export async function readFresh<N extends ReadName>(name: N, ...args: ReadArgs<N>): Promise<ReadResult<N>> {
  const entry = entryFor(name, args);
  await revalidate(entry);
  if (entry.stale) await revalidate(entry);
  return settledValue<N>(entry);
}

/** Warm the cache for a screen the user is likely to open next. */
export function prefetch<N extends ReadName>(name: N, ...args: ReadArgs<N>): void {
  void fetchQuery(name, ...args).catch(() => {});
}

/** Put a mutation's own answer in the cache, as if it had been read. */
export function setQueryValue<N extends ReadName>(name: N, args: ReadArgs<N>, value: ReadResult<N>): void {
  const entry = entryFor(name, args);
  entry.value = value;
  entry.error = null;
  entry.fetchedAt = Date.now();
  entry.stale = false;
}

export interface QueryHandle<T> {
  /** The answer to show, kept while a re-read is in flight; `undefined`
   *  until the first one lands. */
  readonly value: T | undefined;
  /** Why the latest read failed, or `null`. */
  readonly error: string | null;
  /** Reading with nothing to show yet: the placeholder's cue. */
  readonly loading: boolean;
  /** Any read in flight, including a background refresh. */
  readonly refreshing: boolean;
  /** Read again now (joining a read already in flight). */
  reload(): Promise<void>;
}

/**
 * A component's view of one read. `args` is reactive: returning new
 * arguments switches entries, returning `null` shows nothing. Call it during
 * component setup; while the component is mounted the entry counts as
 * observed, so an invalidation re-reads it at once.
 */
export function query<N extends ReadName>(
  name: N,
  args: () => ReadArgs<N> | null | undefined,
): QueryHandle<ReadResult<N>> {
  const key = $derived.by(() => {
    const current = args();
    return current ? keyOf(name, current) : null;
  });
  /** Bumped once the effect has created the entry, so a first read that
   *  found nothing looks again. */
  let created = $state(0);
  const entry = $derived.by(() => {
    void created;
    return key ? (entries.get(key) ?? null) : null;
  });

  $effect(() => {
    if (!key) return;
    const current = untrack(args);
    if (!current) return;
    const observed = entryFor(name, current);
    observed.observers++;
    untrack(() => {
      created++;
      void revalidate(observed);
    });
    return () => {
      observed.observers--;
      observed.usedAt = Date.now();
    };
  });

  return {
    get value() {
      const current = entry;
      if (!current) return undefined;
      const value = current.value;
      return current.has() ? (value as ReadResult<N>) : undefined;
    },
    get error() {
      return entry?.error ?? null;
    },
    get loading() {
      const current = entry;
      if (!current) return key !== null;
      const loading = current.loading;
      return !current.has() && (loading || current.error === null);
    },
    get refreshing() {
      return entry?.loading ?? false;
    },
    reload() {
      return entry ? load(entry) : Promise.resolve();
    },
  };
}

export type QueryMatch = {
  kinds?: readonly ReadKind[];
  repo?: unknown;
  names?: readonly ReadName[];
};

/** Mark matching entries stale; the ones on screen re-read at once. */
export function invalidate(match: QueryMatch): void {
  for (const entry of entries.values()) {
    if (match.kinds && !match.kinds.includes(entry.policy.kind)) continue;
    if (match.names && !match.names.includes(entry.name)) continue;
    // Reads without a repository argument match any repository.
    if (match.repo !== undefined && entry.policy.repoArg !== undefined && entry.repo !== match.repo) continue;
    entry.stale = true;
    if (entry.inflight) entry.rerun = true;
    else if (entry.observers > 0) void load(entry);
  }
}

/** Forget everything. `vitest-setup.ts` calls it before every test. */
export function resetQueryCache(): void {
  entries.clear();
}

if (import.meta.env.MODE === "test") {
  (globalThis as { __resetQueryCache?: () => void }).__resetQueryCache = resetQueryCache;
}

// --- what each mutation makes stale ---------------------------------------

/** Uncached review and git reads: settling one changes nothing. */
const UNCACHED_READS = new Set<string>([
  "gitWorktreeHandoffPreflight",
  "reviewInbox",
  "reviewStacks",
  "reviewAgentReviews",
  "reviewActiveAgentReviews",
  "reviewAgentReviewPrompt",
  "reviewPrLinks",
  "reviewRepository",
  "reviewRepositorySettings",
  "reviewRules",
  "reviewGithubAccount",
  "reviewRepositoryAccount",
  "reviewMyRepositories",
  "reviewRepositoriesRequestingReview",
  "reviewPrsOnlyRepositories",
  "reviewCheckFresh",
  "reviewCheckLogs",
  "reviewLocalDiff",
  "reviewPlanStep",
  "reviewInspectFolder",
  "reviewFindClones",
  "reviewLoadDraft",
  "reviewPendingReview",
  "reviewViewedFiles",
  "reviewFindings",
]);

/** Review mutations that touch less than "this repository on GitHub":
 *  `local` marks local git state, a list marks only those reads. Any review
 *  mutation not listed marks every read of its repository and local state. */
const NARROW: Partial<Record<string, "local" | readonly ReadName[]>> = {
  // The profile database only: nothing cached reads it.
  reviewSetViewed: [],
  reviewSavePendingReview: [],
  reviewDiscardPendingReview: [],
  reviewSaveDraft: [],
  reviewDeleteDraft: [],
  reviewSetFindingState: [],
  reviewMarkSeen: [],
  reviewMarkActivitySeen: [],
  reviewMutePr: [],
  reviewForgetPr: [],
  reviewEnqueueAgentReview: [],
  reviewAgentReviewStarted: [],
  reviewRerunAgentReview: [],
  reviewRemoveAgentReview: [],
  reviewClaimAgentReviews: [],
  reviewRerunOnNewCommits: [],
  reviewRecordAgentReviewResult: [],
  reviewAddRule: [],
  reviewUpdateRule: [],
  reviewRemoveRule: [],
  reviewSaveRepositorySettings: [],
  reviewSetCheckoutPolicy: [],
  reviewAddPrsOnlyRepository: [],
  reviewRemovePrsOnlyRepository: [],
  // Hidden PR refs only; nothing on screen reads them directly.
  reviewFetchPrs: [],
  // Runs after every inbox sync: closed PRs' refs and checkouts.
  reviewPruneClosed: ["reviewCheckoutState"],
  // Thread links and the branches they follow.
  reviewLinkThread: ["reviewBranchProgress"],
  reviewMoveThreadToPr: ["reviewBranchProgress"],
  reviewLinkClone: ["reviewBranchProgress", "reviewProviderStatus"],
  // Review checkouts are worktrees.
  reviewEnsureReviewCheckout: "local",
  reviewPromoteCheckout: "local",
  reviewRemoveReviewCheckout: "local",
  reviewCreatePrWorktree: "local",
  reviewRunCheckoutSetup: "local",
  reviewCheckOutThreadCommit: "local",
  reviewCloneRepository: [],
};

/** Whether a settled command could have changed git or GitHub state. */
export function isMutation(name: string): boolean {
  return (name.startsWith("git") || name.startsWith("review")) && !(name in READS) && !UNCACHED_READS.has(name);
}

/** Whether a settled command could have changed the working tree, refs or
 *  worktrees: any git mutation, and the review mutations that run git. */
export function touchesLocal(name: string): boolean {
  if (!isMutation(name)) return false;
  if (name.startsWith("git")) return true;
  const narrow = NARROW[name];
  return narrow === undefined || narrow === "local";
}

export function invalidateAfter(command: SettledCommand): void {
  const { name } = command;
  if (!isMutation(name)) return;
  if (name.startsWith("git")) {
    invalidate({ kinds: ["local"] });
    return;
  }
  if (!name.startsWith("review")) return;
  if (name === "reviewSwitchGithubAccount") {
    invalidate({ kinds: ["github", "provider"] });
    return;
  }
  const repo = argNamed(command, "repoDir");
  const narrow = NARROW[name];
  if (narrow === "local") {
    invalidate({ kinds: ["local"] });
  } else if (narrow) {
    if (narrow.length > 0) invalidate({ names: narrow, ...(repo === undefined ? {} : { repo }) });
  } else {
    invalidate({ kinds: ["local"] });
    if (typeof repo === "string") invalidate({ kinds: ["github"], repo });
  }
}

onCommandSettled(invalidateAfter);

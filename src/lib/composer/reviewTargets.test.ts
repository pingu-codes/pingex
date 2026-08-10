import { describe, expect, it } from "vitest";
import { filterBranches, filterCommits, filterModes, REVIEW_MODES } from "$lib/composer/reviewTargets";
import type { GitBranch, GitCommit } from "$lib/types";

const branches: GitBranch[] = [
  { name: "origin/main", isRemote: true, isCurrent: false },
  { name: "feature/maintenance", isRemote: false, isCurrent: false },
  { name: "main", isRemote: false, isCurrent: true },
];

const commits: GitCommit[] = [
  {
    hash: "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b",
    shortHash: "1a2b3c4",
    subject: "feat: review picker",
    author: "Ciaran Kelly",
    timestamp: 1,
  },
  {
    hash: "2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c",
    shortHash: "2b3c4d5",
    subject: "fix: thread scroll",
    author: "Ciaran Kelly",
    timestamp: 2,
  },
];

describe("filterModes", () => {
  it("offers every mode for an empty query", () => {
    expect(filterModes("")).toEqual(REVIEW_MODES);
    expect(filterModes("  ")).toEqual(REVIEW_MODES);
  });

  it("matches on the label", () => {
    expect(filterModes("branch").map((mode) => mode.id)).toEqual(["baseBranch"]);
    expect(filterModes("uncommitted").map((mode) => mode.id)).toEqual(["uncommittedChanges"]);
  });

  it("comes back empty for a query nothing matches", () => {
    expect(filterModes("zzz")).toEqual([]);
  });
});

describe("filterBranches", () => {
  it("keeps the given order for an empty query", () => {
    expect(filterBranches(branches, "")).toEqual(branches);
  });

  it("puts a prefix match ahead of one further in the name", () => {
    // `main` is a prefix of `main` only; the other two merely contain it.
    expect(filterBranches(branches, "main").map((branch) => branch.name)).toEqual([
      "main",
      "origin/main",
      "feature/maintenance",
    ]);
  });

  it("matches the remote prefix too", () => {
    expect(filterBranches(branches, "origin/").map((branch) => branch.name)).toEqual(["origin/main"]);
  });
});

describe("filterCommits", () => {
  it("matches a subject", () => {
    expect(filterCommits(commits, "scroll").map((commit) => commit.shortHash)).toEqual(["2b3c4d5"]);
  });

  it("matches a hash prefix", () => {
    expect(filterCommits(commits, "1a2b").map((commit) => commit.shortHash)).toEqual(["1a2b3c4"]);
  });

  it("keeps everything for an empty query", () => {
    expect(filterCommits(commits, "")).toEqual(commits);
  });
});

// The Changes page in the browser preview: every thread works in a Stack
// worktree of `api` (two PRs, the upper one checked out, with uncommitted
// edits) and on a git branch of `web`. Diffs are made up per path.

import type { commands } from "$lib/bindings";
import type { ChangeSegment, ChangeSegmentFile, PrFile, RepoChanges, ThreadChanges } from "$lib/types";
import { addedFile } from "./factories";

type Commands = typeof commands;

const API = "/repos/api";
const WEB = "/repos/web";
const sha = (seed: string) => seed.repeat(40).slice(0, 40);

function listed(path: string, additions: number, deletions: number, status = "modified"): ChangeSegmentFile {
  return {
    path,
    oldPath: null,
    status,
    additions,
    deletions,
    binary: false,
    contentKey: `${path}:${additions}:${deletions}`,
    beforeThread: false,
  };
}

function segment(
  id: string,
  extra: Partial<ChangeSegment> & Pick<ChangeSegment, "label" | "baseLabel" | "files">,
): ChangeSegment {
  return {
    id,
    kind: "branch",
    branch: null,
    prNumber: null,
    baseSha: sha("b"),
    headSha: sha("c"),
    current: false,
    rewritten: false,
    needsRestack: false,
    note: null,
    totalFiles: extra.files.length,
    truncated: false,
    additions: extra.files.reduce((sum, file) => sum + file.additions, 0),
    deletions: extra.files.reduce((sum, file) => sum + file.deletions, 0),
    ...extra,
  };
}

function repos(): RepoChanges[] {
  return [
    {
      root: API,
      label: "api",
      branch: "billing/charge",
      headSha: sha("2"),
      inProgress: null,
      purpose: {
        kind: "stack",
        repoDir: API,
        number: 7,
        name: "billing",
        baseRef: "main",
        prs: [
          { number: 41, title: "Ledger model", headRef: "billing/ledger" },
          { number: 42, title: "Charge customers", headRef: "billing/charge" },
        ],
      },
      segments: [
        segment("pr:41", {
          kind: "stackPr",
          label: "#41 Ledger model",
          branch: "billing/ledger",
          prNumber: 41,
          baseLabel: "origin/main",
          baseSha: sha("0"),
          headSha: sha("1"),
          files: [listed("src/ledger/model.ts", 18, 0, "added"), listed("src/ledger/index.ts", 2, 0, "added")],
        }),
        segment("pr:42", {
          kind: "stackPr",
          label: "#42 Charge customers",
          branch: "billing/charge",
          prNumber: 42,
          baseLabel: "#41",
          baseSha: sha("1"),
          headSha: sha("2"),
          current: true,
          files: [listed("src/billing/charge.ts", 24, 3), listed("src/billing/charge.test.ts", 30, 0, "added")],
        }),
        segment("uncommitted", {
          kind: "uncommitted",
          label: "Uncommitted",
          branch: "billing/charge",
          baseLabel: "HEAD",
          baseSha: sha("2"),
          headSha: null,
          current: true,
          files: [listed("src/billing/charge.ts", 4, 1), listed("notes/plan.md", 6, 0, "untracked")],
        }),
      ],
      error: null,
      note: null,
    },
    {
      root: WEB,
      label: "web",
      branch: "billing-ui",
      headSha: sha("9"),
      inProgress: null,
      purpose: null,
      segments: [
        segment("branch:billing-ui", {
          label: "billing-ui",
          branch: "billing-ui",
          baseLabel: "origin/main",
          baseSha: sha("8"),
          headSha: sha("9"),
          current: true,
          files: [listed("src/routes/billing/+page.svelte", 40, 6)],
        }),
      ],
      error: null,
      note: null,
    },
  ];
}

function modified(path: string, added: string[], removed: string[]): PrFile {
  const header = `@@ -1,${removed.length + 1} +1,${added.length + 1} @@`;
  const lines = [
    { kind: "context" as const, content: "// unchanged", oldLine: 1, newLine: 1 },
    ...removed.map((content, index) => ({ kind: "del" as const, content, oldLine: index + 2, newLine: null })),
    ...added.map((content, index) => ({ kind: "add" as const, content, oldLine: null, newLine: index + 2 })),
  ];
  return {
    path,
    oldPath: null,
    status: "modified",
    additions: added.length,
    deletions: removed.length,
    patch: [header, " // unchanged", ...removed.map((line) => `-${line}`), ...added.map((line) => `+${line}`)].join(
      "\n",
    ),
    patchTruncated: false,
    blobSha: null,
    hunks: [
      {
        header,
        oldStart: 1,
        oldLines: removed.length + 1,
        newStart: 1,
        newLines: added.length + 1,
        lines,
      },
    ],
  };
}

function patchFor(file: ChangeSegmentFile): PrFile {
  if (file.status === "added" || file.status === "untracked") {
    return addedFile(file.path, [`// ${file.path}`, "export const value = 1;"]);
  }
  return modified(
    file.path,
    ["export function charge(amount: number) {", "  return ledger.add(amount);", "}"],
    ["export function charge() {}"],
  );
}

function findSegment(root: string, base: string, head: string | null): ChangeSegment | undefined {
  return repos()
    .find((repo) => repo.root === root)
    ?.segments.find((entry) => entry.baseSha === base && entry.headSha === head);
}

export const changesCommands = {
  gitThreadChanges: (_threadId, _cwd, scope): ThreadChanges => ({
    scope,
    repos:
      scope === "thread"
        ? repos().map((repo) => ({
            ...repo,
            segments: repo.segments.filter((entry) => entry.current),
          }))
        : repos(),
    snapshotAt: scope === "thread" ? Math.floor(Date.now() / 1000) - 600 : null,
    readAt: Date.now(),
  }),
  gitSegmentDiff: (repoDir, baseSha, headSha) => ({
    files: (findSegment(repoDir, baseSha, headSha)?.files ?? []).map(patchFor),
    truncated: false,
  }),
  gitWorkingDiff: (repoDir, baseSha) => ({
    files: (findSegment(repoDir, baseSha, null)?.files ?? []).map(patchFor),
    truncated: false,
  }),
} satisfies {
  [K in "gitThreadChanges" | "gitSegmentDiff" | "gitWorkingDiff"]: (
    ...args: Parameters<Commands[K]>
  ) => Awaited<ReturnType<Commands[K]>>;
};

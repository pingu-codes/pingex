import { afterEach, describe, expect, it, vi } from "vitest";
import type { ChangeSegment, ChangeSegmentFile, RepoChanges, ThreadChanges } from "$lib/types";
import { pickSegment, RefreshScheduler, segmentFiles, segmentKey, totals, viewedKey } from "./changesPage";

function file(path: string, extra: Partial<ChangeSegmentFile> = {}): ChangeSegmentFile {
  return {
    path,
    oldPath: null,
    status: "modified",
    additions: 1,
    deletions: 0,
    binary: false,
    contentKey: `blob-${path}`,
    beforeThread: false,
    ...extra,
  };
}

function segment(id: string, extra: Partial<ChangeSegment> = {}): ChangeSegment {
  return {
    id,
    kind: id === "uncommitted" ? "uncommitted" : "stackPr",
    label: id,
    branch: null,
    prNumber: null,
    baseLabel: "origin/main",
    baseSha: "b".repeat(40),
    headSha: id === "uncommitted" ? null : "h".repeat(40),
    current: false,
    rewritten: false,
    needsRestack: false,
    note: null,
    files: [],
    totalFiles: 0,
    truncated: false,
    additions: 0,
    deletions: 0,
    ...extra,
  };
}

function repo(root: string, segments: ChangeSegment[]): RepoChanges {
  return {
    root,
    label: root,
    branch: null,
    headSha: null,
    inProgress: null,
    purpose: null,
    segments,
    error: null,
    note: null,
  };
}

function changes(...repos: RepoChanges[]): ThreadChanges {
  return { scope: "branch", repos, snapshotAt: null, readAt: 0 };
}

describe("pickSegment", () => {
  const stack = changes(
    repo("/api", [
      segment("pr:1", { files: [file("a.ts")] }),
      segment("pr:2", { current: true, files: [file("b.ts")] }),
      segment("uncommitted", { current: true, files: [file("c.ts")] }),
    ]),
    repo("/web", [segment("branch:feat", { current: true, files: [file("a.ts")] })]),
  );

  it("opens on the working tree, else the checked-out branch", () => {
    expect(pickSegment(stack, null)?.segment.id).toBe("uncommitted");
    const clean = changes(repo("/api", [segment("pr:1"), segment("pr:2", { current: true })]));
    expect(pickSegment(clean, null)?.segment.id).toBe("pr:2");
  });

  it("keeps the chosen segment across reads, by repository and id", () => {
    expect(pickSegment(stack, segmentKey("/web", "branch:feat"))?.repo.root).toBe("/web");
    expect(pickSegment(stack, "pr:1")?.segment.id).toBe("pr:1");
    // Gone: falls back rather than showing nothing.
    expect(pickSegment(stack, segmentKey("/api", "pr:9"))?.segment.id).toBe("uncommitted");
  });

  it("finds the segment holding a file it was opened on", () => {
    expect(pickSegment(stack, null, "b.ts")?.segment.id).toBe("pr:2");
  });

  it("has nothing to pick when nothing changed", () => {
    expect(pickSegment(changes(repo("/api", [])), null)).toBeNull();
    expect(pickSegment(null, null)).toBeNull();
  });
});

describe("segmentFiles", () => {
  it("keeps the listing's order and marks files the diff left out", () => {
    const listed = segment("uncommitted", {
      files: [file("z.ts"), file("new.ts", { status: "untracked" }), file("big.bin")],
    });
    const patched = [
      {
        path: "new.ts",
        oldPath: null,
        status: "added",
        additions: 1,
        deletions: 0,
        patch: "+x",
        hunks: [],
        patchTruncated: false,
        blobSha: null,
      },
      {
        path: "z.ts",
        oldPath: null,
        status: "modified",
        additions: 1,
        deletions: 0,
        patch: "+y",
        hunks: [],
        patchTruncated: false,
        blobSha: null,
      },
    ];
    const files = segmentFiles(listed, patched);
    expect(files.map((entry) => entry.path)).toEqual(["z.ts", "new.ts", "big.bin"]);
    expect(files[1].status).toBe("added");
    expect(files[2].patchTruncated).toBe(true);
    // Before the diff lands, nothing claims to be cut.
    expect(segmentFiles(listed, undefined).every((entry) => !entry.patchTruncated)).toBe(true);
  });
});

describe("viewedKey", () => {
  it("changes when the file's contents do", () => {
    const at = segment("uncommitted");
    expect(viewedKey("/api", at, file("a.ts"))).toBe(viewedKey("/api", at, file("a.ts")));
    expect(viewedKey("/api", at, file("a.ts", { contentKey: "other" }))).not.toBe(viewedKey("/api", at, file("a.ts")));
    expect(viewedKey("/web", at, file("a.ts"))).not.toBe(viewedKey("/api", at, file("a.ts")));
  });
});

describe("totals", () => {
  it("counts a file once per repository however many segments hold it", () => {
    const summary = totals(
      changes(
        repo("/api", [
          segment("pr:1", { files: [file("a.ts")], additions: 3, deletions: 1 }),
          segment("uncommitted", { files: [file("a.ts"), file("b.ts")], additions: 2, deletions: 0 }),
        ]),
        repo("/web", [segment("branch:x", { files: [file("a.ts")], additions: 1, deletions: 1 })]),
      ),
    );
    expect(summary).toEqual({ files: 3, additions: 6, deletions: 2 });
  });
});

describe("RefreshScheduler", () => {
  afterEach(() => vi.useRealTimers());

  it("waits for a quiet spell, but never past the longest wait", () => {
    vi.useFakeTimers({ toFake: ["Date", "setTimeout", "clearTimeout"] });
    const run = vi.fn();
    const scheduler = new RefreshScheduler(run, 750, 3000);
    scheduler.poke();
    vi.advanceTimersByTime(500);
    scheduler.poke();
    vi.advanceTimersByTime(500);
    expect(run).not.toHaveBeenCalled();
    vi.advanceTimersByTime(300);
    expect(run).toHaveBeenCalledTimes(1);

    // Non-stop pokes still read every three seconds.
    for (let tick = 0; tick < 10; tick++) {
      scheduler.poke();
      vi.advanceTimersByTime(400);
    }
    expect(run).toHaveBeenCalledTimes(2);
    scheduler.dispose();
    vi.advanceTimersByTime(5000);
    expect(run).toHaveBeenCalledTimes(2);
  });
});

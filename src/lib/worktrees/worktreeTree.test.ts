// @vitest-environment node
import { describe, expect, it } from "vitest";
import { threadSummary, worktreeEntry } from "$lib/services/preview/factories";
import type { Project, ThreadSummary } from "$lib/types";
import { nestProjects } from "$lib/worktrees/worktreeTree";

function project(path: string, overrides: Partial<Project> = {}): Project {
  return {
    path,
    name: path.split("/").pop() ?? path,
    kind: "folder",
    workspaceId: null,
    archived: false,
    instructions: "",
    sources: [],
    pinned: false,
    expanded: true,
    threads: [],
    members: [],
    recencyAt: null,
    ...overrides,
  };
}

const thread = (id: string, cwd: string, updatedAt = 1): ThreadSummary =>
  threadSummary({ id, title: id, cwd, updatedAt });

const ids = (threads: ThreadSummary[]) => threads.map((entry) => entry.id);

describe("nestProjects", () => {
  const main = worktreeEntry({ path: "/repo", isMain: true, branch: "main" });

  it("moves an adopted worktree project under its repository", () => {
    const repo = project("/repo");
    const adopted = project("/wt/feature", { kind: "worktree", name: "Feature" });
    const nested = nestProjects([repo, adopted], { "/repo": [main, worktreeEntry({ path: "/wt/feature" })] }, []);
    expect(nested.roots.map((root) => root.path)).toEqual(["/repo"]);
    expect(nested.children["/repo"]).toEqual([
      expect.objectContaining({ path: "/wt/feature", name: "Feature", branch: "feature", adopted }),
    ]);
    expect(nested.unaddedParents).toEqual({});
  });

  it("files unfiled threads under an unadded worktree outside the repo", () => {
    const nested = nestProjects(
      [project("/repo", { threads: [thread("repo-thread", "/repo")] })],
      { "/repo": [main, worktreeEntry({ path: "/elsewhere/x-wt", branch: "x" })] },
      [thread("stray", "/elsewhere/x-wt/src"), thread("lost", "/nowhere")],
    );
    const [child] = nested.children["/repo"];
    expect(child).toMatchObject({ path: "/elsewhere/x-wt", name: "x-wt", branch: "x", adopted: null });
    expect(ids(child.threads)).toEqual(["stray"]);
    expect(ids(nested.roots[0].threads)).toEqual(["repo-thread"]);
    expect(nested.unaddedParents).toEqual({ "/elsewhere/x-wt": "/repo" });
  });

  it("takes threads in a worktree inside the repo away from the repo row", () => {
    const repo = project("/repo", {
      threads: [thread("root", "/repo", 3), thread("in-wt", "/repo/.claude/worktrees/y", 2)],
    });
    const nested = nestProjects([repo], { "/repo": [main, worktreeEntry({ path: "/repo/.claude/worktrees/y" })] }, []);
    expect(ids(nested.roots[0].threads)).toEqual(["root"]);
    expect(ids(nested.children["/repo"][0].threads)).toEqual(["in-wt"]);
    // The original project is left untouched for the flat view.
    expect(ids(repo.threads)).toEqual(["root", "in-wt"]);
  });

  it("files a thread under the deepest matching worktree", () => {
    const nested = nestProjects(
      [project("/repo")],
      { "/repo": [main, worktreeEntry({ path: "/wt/a" }), worktreeEntry({ path: "/wt/a/b" })] },
      [thread("deep", "/wt/a/b/c"), thread("shallow", "/wt/a/c")],
    );
    const byPath = Object.fromEntries(nested.children["/repo"].map((child) => [child.path, ids(child.threads)]));
    expect(byPath).toEqual({ "/wt/a": ["shallow"], "/wt/a/b": ["deep"] });
  });

  it("leaves a worktree project at the root when its repository is not listed", () => {
    const orphan = project("/wt/feature", { kind: "worktree" });
    const nested = nestProjects([orphan], {}, []);
    expect(nested.roots).toEqual([orphan]);
    expect(nested.children).toEqual({});
  });

  it("skips temporary, missing and stale worktrees", () => {
    const nested = nestProjects(
      [project("/repo")],
      {
        "/repo": [
          main,
          worktreeEntry({ path: "/tmp/a", isTemp: true }),
          worktreeEntry({ path: "/gone", missingDir: true }),
          worktreeEntry({ path: "/stale", prunable: true }),
        ],
      },
      [],
    );
    expect(nested.children).toEqual({});
  });
});

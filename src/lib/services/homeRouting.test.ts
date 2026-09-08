import { afterEach, describe, expect, it, vi } from "vitest";
import { type BootstrapData, type HomeSnapshot, type Host, commands as native } from "$lib/bindings";
import {
  acceptProfile,
  commands,
  identity,
  localProjectPath,
  scopedId,
  scopeEvent,
  setHomeSelection,
} from "./homeRouting";

function home(key: string, host: Host, harness: "codex" | "claude", cwd: string): HomeSnapshot {
  const data: BootstrapData = {
    codexHome: "/config",
    codexBinary: "codex",
    account: null,
    sideQuestions: [],
    threadBranches: [],
    subagents: [],
    sections: [],
    sectionsSupported: false,
    sidebarLayout: { folders: [], placements: [] },
    projects: [
      {
        path: cwd,
        name: "Project",
        kind: "folder",
        workspaceId: null,
        pinned: false,
        archived: false,
        expanded: true,
        instructions: "",
        sources: [],
        members: [],
        threads: [
          {
            id: "same-thread-id",
            cwd,
            title: "Thread",
            updatedAt: 1,
            status: "idle",
            pinned: false,
            parentThreadId: null,
            agentNickname: null,
            agentRole: null,
            projectId: null,
            sectionId: null,
            subagentCount: 0,
            hidden: false,
            harness,
          },
        ],
      },
    ],
  };
  return {
    homeKey: key,
    home: { id: key, host, harness, configDir: "/config", binary: harness, label: key, isDefault: true },
    data,
    error: null,
  };
}

const nativeHost: Host = { kind: "native" };
const ubuntu: Host = { kind: "wsl", distro: "Ubuntu" };

afterEach(() => {
  vi.restoreAllMocks();
  setHomeSelection(() => ({}));
});

describe("Home routing", () => {
  it("rejects cross-Host workspace members before IPC", async () => {
    acceptProfile({
      profileKey: "win",
      homes: [home("win", nativeHost, "codex", "C:\\repo"), home("linux", ubuntu, "codex", "/repo")],
    });
    const create = vi.spyOn(native, "createWorkspace");
    await expect(
      commands.createWorkspace({
        name: "Mixed",
        members: [
          { alias: "win", sourcePath: "C:\\repo", isolated: false },
          { alias: "linux", sourcePath: localProjectPath(ubuntu, "/repo"), isolated: false },
        ],
      }),
    ).rejects.toThrow("same Host");
    expect(create).not.toHaveBeenCalled();
  });

  it("keeps an existing thread on its Home after defaults change", async () => {
    const original = home("original", nativeHost, "codex", "C:\\repo");
    original.home.isDefault = false;
    const replacement = home("replacement", nativeHost, "codex", "C:\\repo");
    acceptProfile({ profileKey: "original", homes: [original, replacement] });
    const read = vi.spyOn(native, "readThread").mockResolvedValue({ id: "same-thread-id" });
    const start = vi.spyOn(native, "startThread").mockResolvedValue({ id: "new" });
    await commands.readThread(scopedId("original", "same-thread-id"));
    await commands.startThread("C:\\repo", null, false, "codex");
    expect(read).toHaveBeenCalledWith("same-thread-id", { homeKey: "original" });
    expect(start).toHaveBeenCalledWith("C:\\repo", null, false, "codex", { homeKey: "replacement" });
  });

  it("keeps shared sidebar Windows paths intact", async () => {
    const root = home("root", nativeHost, "codex", "C:\\repo");
    root.data.sidebarLayout.placements.push({ scope: "", itemKey: "C:\\repo", parentId: null, ordinal: 0 });
    const data = acceptProfile({ profileKey: "root", homes: [root] });
    expect(data.sidebarLayout.placements[0].itemKey).toBe("C:\\repo");
  });
  it("keeps identical native conversation IDs distinct across both harnesses and Hosts", async () => {
    const homes = [
      home("win-codex", nativeHost, "codex", "C:\\repo"),
      home("win-claude", nativeHost, "claude", "C:\\repo"),
      home("wsl-codex", ubuntu, "codex", "/repo"),
      home("wsl-claude", ubuntu, "claude", "/repo"),
    ];
    const data = acceptProfile({ profileKey: "profile", homes });
    expect(data.projects).toHaveLength(2);
    const ids = data.projects.flatMap((project) => project.threads.map((thread) => thread.id));
    expect(new Set(ids).size).toBe(4);
    const read = vi.spyOn(native, "readThread").mockResolvedValue({ id: "same-thread-id", turns: [] });
    await Promise.all(ids.map((id) => commands.readThread(id)));
    for (const entry of homes) expect(read).toHaveBeenCalledWith("same-thread-id", { homeKey: entry.homeKey });
  });

  it("starts each harness inside the project's Host without rewriting the prompt", async () => {
    acceptProfile({
      profileKey: "profile",
      homes: [home("win", nativeHost, "codex", "C:\\repo"), home("linux", ubuntu, "claude", "/repo")],
    });
    const start = vi.spyOn(native, "startThread").mockResolvedValue({ id: "new", cwd: "/repo" });
    const created = (await commands.startThread(localProjectPath(ubuntu, "/repo"), null, false, "claude")) as {
      id: string;
    };
    expect(start).toHaveBeenCalledWith("/repo", null, false, "claude", { homeKey: "linux" });
    expect(identity(created.id)).toEqual({ homeKey: "linux", nativeId: "new" });
    const turn = vi.spyOn(native, "startTurn").mockResolvedValue({ id: "turn" });
    const input = [{ type: "text", text: created.id }];
    await commands.startTurn(created.id, input, null);
    expect(turn).toHaveBeenCalledWith("new", input, null, { homeKey: "linux" });
  });

  it("routes colliding approval IDs independently after navigation changes", async () => {
    acceptProfile({
      profileKey: "profile",
      homes: [home("win", nativeHost, "codex", "C:\\repo"), home("linux", ubuntu, "codex", "/repo")],
    });
    const a = scopeEvent("win", { requestId: 7, threadId: "same-thread-id" });
    const b = scopeEvent("linux", { requestId: 7, threadId: "same-thread-id" });
    expect(a.requestId).not.toBe(b.requestId);
    setHomeSelection(() => ({ projectPath: "C:\\repo" }));
    const respond = vi.spyOn(native, "respondApproval").mockResolvedValue(null);
    await commands.respondApproval(b.requestId, "accept");
    await commands.respondApproval(a.requestId, "decline");
    expect(respond.mock.calls).toEqual([
      [7, "accept", { homeKey: "linux" }],
      [7, "decline", { homeKey: "win" }],
    ]);
  });

  it("does not alias /mnt/c or identical paths in different distributions", () => {
    expect(localProjectPath(ubuntu, "/mnt/c/repo")).not.toBe("C:\\repo");
    expect(localProjectPath(ubuntu, "/repo")).not.toBe(localProjectPath({ kind: "wsl", distro: "Debian" }, "/repo"));
    expect(identity(scopedId("a:[]", "id:[]"))).toEqual({ homeKey: "a:[]", nativeId: "id:[]" });
  });
});

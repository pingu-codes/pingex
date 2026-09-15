import type { MemoryHome } from "$lib/types";

/** Memory files for the browser preview, edited in memory. */
const files: Record<string, string> = {
  "/home/you/.codex/memories/MEMORY.md":
    "# Memory\n\n- The frontend uses Skeleton for components.\n- Run `deno task check` before finishing.\n",
  "/home/you/.codex/memories/rollout_summaries/sidebar-folders.md":
    "Added drag-and-drop sidebar folders; ordering persists per scope.\n",
  "/home/you/.claude/projects/-home-you-codex-custom/memory/MEMORY.md":
    "# Memory index\n\n- [Verification gotchas](verification-gotchas.md) — lint:fix touches report.html\n",
  "/home/you/.claude/projects/-home-you-codex-custom/memory/verification-gotchas.md":
    "---\nname: verification-gotchas\ndescription: lint:fix touches report.html\nmetadata:\n  type: feedback\n---\n\nBare `cargo fmt` reformats untouched files; format only what you changed.\n",
};

const toggles: Record<string, Record<string, boolean>> = {
  codex: { "features.memories": true, "memories.generate_memories": true, "memories.use_memories": true },
  claude: { autoMemoryEnabled: true },
};

const LABELS: Record<string, [string, string]> = {
  "features.memories": ["Memories", "Let Codex remember things between threads. Off by default in Codex."],
  "memories.generate_memories": ["Generate memories", "Extract memories from finished threads."],
  "memories.use_memories": ["Use memories", "Give new threads the memories Codex has consolidated."],
  autoMemoryEnabled: ["Auto memory", "Let Claude Code save and recall memories for each project."],
};

function entries(root: string) {
  return Object.keys(files)
    .filter((path) => path.startsWith(`${root}/`))
    .sort()
    .map((path) => {
      const relPath = path.slice(root.length + 1);
      const body = files[path];
      const name = body.match(/^name: (.+)$/m)?.[1];
      const description = body.match(/^description: (.+)$/m)?.[1];
      const firstLine = body
        .replace(/^---[\s\S]*?\n---\n/, "")
        .split("\n")
        .find((line) => line.trim())
        ?.replace(/^#+\s*/, "");
      const isIndex = relPath === "MEMORY.md";
      return {
        path,
        relPath,
        title: name ?? relPath.replace(/^.*\//, "").replace(/\.md$/, ""),
        description: description ?? firstLine ?? null,
        kind: isIndex
          ? "index"
          : (body.match(/^\s*type: (.+)$/m)?.[1] ?? (relPath.startsWith("rollout_summaries/") ? "rollout" : null)),
        modifiedAt: Date.now() / 1000 - 3600,
        bytes: body.length,
        unindexed: root.includes("/projects/") && !isIndex && !files[`${root}/MEMORY.md`]?.includes(`](${relPath})`),
      };
    });
}

function home(homeId: string, label: string, configDir: string, stores: MemoryHome["stores"]): MemoryHome {
  return {
    homeId,
    label,
    configDir,
    error: null,
    toggles: Object.entries(toggles[homeId]).map(([key, value]) => ({
      key,
      label: LABELS[key][0],
      description: LABELS[key][1],
      value,
    })),
    stores,
  };
}

export function previewListMemories(): MemoryHome[] {
  const claudeRoot = "/home/you/.claude/projects/-home-you-codex-custom/memory";
  return [
    home("codex", "Codex", "/home/you/.codex", [
      {
        root: "/home/you/.codex/memories",
        title: "Memories",
        projectPath: null,
        entries: entries("/home/you/.codex/memories"),
      },
    ]),
    home("claude", "Claude Code", "/home/you/.claude", [
      { root: claudeRoot, title: "codex-custom", projectPath: "/home/you/codex-custom", entries: entries(claudeRoot) },
    ]),
  ];
}

export function previewReadMemory(path: string): string {
  const body = files[path];
  if (body === undefined) throw new Error(`${path} is not a memory file.`);
  return body;
}

export function previewWriteMemory(path: string, contents: string): MemoryHome[] {
  previewReadMemory(path);
  files[path] = contents;
  return previewListMemories();
}

export function previewDeleteMemory(path: string): MemoryHome[] {
  previewReadMemory(path);
  delete files[path];
  const slash = path.lastIndexOf("/");
  const index = `${path.slice(0, slash)}/MEMORY.md`;
  const rel = path.slice(slash + 1);
  if (path.includes("/projects/") && files[index]) {
    files[index] = files[index]
      .split("\n")
      .filter((line) => !line.includes(`](${rel})`))
      .join("\n");
  }
  return previewListMemories();
}

export function previewSetMemoryToggle(homeId: string, key: string, value: boolean): MemoryHome[] {
  toggles[homeId][key] = value;
  return previewListMemories();
}

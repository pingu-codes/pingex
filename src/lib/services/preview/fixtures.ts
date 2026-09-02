import type {
  AccountRateLimits,
  AgentRun,
  AgentSettings,
  ArchivedThread,
  BootstrapData,
  ConfigSetting,
  FileHit,
  GitBranch,
  GitCommit,
  GitRepoInfo,
  GitStatus,
  HomeOverview,
  IntegrationsList,
  LaunchState,
  McpServerStatus,
  McpServerSummary,
  Model,
  PrDetail,
  ProjectSource,
  ProviderStatus,
  PrSummary,
  QueuedSubmission,
  RemoteConnection,
  RuntimeSettings,
  ThreadDetail,
  ThreadSearchFilter,
  ThreadSearchItem,
  ThreadSearchPage,
  ThreadSummary,
  ThreadsPage,
  ThreadUsage,
  WireMessage,
  WorkspaceSearchResults,
  WorktreeEntry,
} from "$lib/types";

let previewCounter = 100;

export function nextPreviewId(): number {
  return previewCounter++;
}

export const previewData: BootstrapData = {
  codexHome: "~/.codex-personal",
  codexBinary: "codex",
  account: { label: "ciaran@example.com", plan: "Pro", kind: "chatgpt" },
  projects: [
    {
      name: "codex-custom",
      path: "/Users/ciaran/Projects/codex-custom",
      kind: "folder",
      workspaceId: null,
      archived: false,
      pinned: false,
      expanded: true,
      instructions: "Prefer small, focused PRs. Keep IPC in api.ts and never walk the filesystem from the renderer.",
      sources: [
        {
          id: "src-preview-1",
          projectPath: "/Users/ciaran/Projects/codex-custom",
          sourcePath: "/Users/ciaran/Projects/codex-custom/docs",
          kind: "folder",
          addedAt: Date.now() / 1000 - 3600,
          status: "indexed",
          indexedAt: Date.now() / 1000 - 3500,
          docCount: 42,
          error: null,
        },
        {
          id: "src-preview-2",
          projectPath: "/Users/ciaran/Projects/codex-custom",
          sourcePath: "/Users/ciaran/Projects/codex-custom/AGENTS.md",
          kind: "file",
          addedAt: Date.now() / 1000 - 1800,
          status: "pending",
          indexedAt: null,
          docCount: 0,
          error: null,
        },
      ],
      threads: [
        {
          id: "1",
          cwd: "",
          title: "Custom frontend skeleton",
          updatedAt: Date.now() / 1000,
          status: "idle",
          pinned: false,
          parentThreadId: null,
          agentNickname: null,
          agentRole: null,
          projectId: null,
          subagentCount: 2,
          hidden: false,
          sectionId: "section-focus",
        },
        {
          id: "2",
          cwd: "",
          title: "Tauri app-server bridge",
          updatedAt: Date.now() / 1000 - 4800,
          status: "idle",
          pinned: false,
          parentThreadId: null,
          agentNickname: null,
          agentRole: null,
          projectId: null,
          sectionId: null,
          subagentCount: 0,
          hidden: false,
        },
      ],
    },
    {
      name: "arctic-explorer",
      path: "/Users/ciaran/Projects/arctic-explorer",
      kind: "folder",
      workspaceId: null,
      archived: false,
      instructions: "",
      sources: [],
      pinned: false,
      expanded: true,
      threads: [
        {
          id: "3",
          cwd: "",
          title: "Improve analysis console",
          updatedAt: Date.now() / 1000 - 86400,
          status: "idle",
          pinned: false,
          parentThreadId: null,
          agentNickname: null,
          agentRole: null,
          projectId: null,
          subagentCount: 0,
          hidden: false,
          sectionId: "section-focus",
        },
        // Enough threads to push this project past the sidebar's per-project cap
        // so the "Show N more" control is exercisable in preview.
        ...Array.from({ length: 18 }, (_, index) => ({
          id: `arctic-${index + 1}`,
          cwd: "",
          title: `Ice core batch ${index + 1}`,
          updatedAt: Date.now() / 1000 - 90000 - index * 3600,
          status: "idle" as const,
          pinned: false,
          parentThreadId: null,
          agentNickname: null,
          agentRole: null,
          projectId: null,
          sectionId: null,
          subagentCount: 0,
          hidden: false,
        })),
      ],
    },
    {
      name: "search-ranking",
      path: "/Users/ciaran/.codex/worktrees/0357/search-ranking",
      kind: "worktree",
      workspaceId: null,
      archived: false,
      instructions: "",
      sources: [],
      pinned: false,
      expanded: true,
      threads: [],
    },
  ],
  sideQuestions: [
    { sideThreadId: "side-1", parentThreadId: "1", title: "Why trailing edge?", createdAt: Date.now() / 1000 - 600 },
  ],
  threadBranches: [],
  subagents: [
    {
      id: "agent-1",
      parentThreadId: "1",
      cwd: "/Users/ciaran/Projects/codex-custom",
      title: "Inspect composer state",
      updatedAt: Date.now() / 1000 - 30,
      status: "active",
      pinned: false,
      projectId: null,
      sectionId: null,
      agentNickname: "Scout",
      agentRole: "researcher",
      subagentCount: 1,
      hidden: false,
    },
    {
      id: "agent-2",
      parentThreadId: "agent-1",
      cwd: "/Users/ciaran/Projects/codex-custom",
      title: "Review UI behavior",
      updatedAt: Date.now() / 1000 - 15,
      status: "completed",
      pinned: false,
      projectId: null,
      sectionId: null,
      agentNickname: "Reviewer",
      agentRole: "reviewer",
      subagentCount: 0,
      hidden: false,
    },
  ],
  sections: [
    { id: "section-focus", name: "This week", icon: null, color: "#f59e0b" },
    { id: "section-later", name: "Later", icon: null, color: null },
  ],
  sectionsSupported: true,
  sidebarLayout: { folders: [], placements: [] },
};

export function previewSort(): BootstrapData {
  previewData.projects.sort((a, b) => Number(b.pinned) - Number(a.pinned));
  for (const project of previewData.projects) {
    project.threads.sort((a, b) => Number(b.pinned) - Number(a.pinned));
  }
  return previewData;
}

export const previewPixel =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

const previewLongDiff = [
  "@@ -1,400 +1,400 @@",
  ...Array.from({ length: 400 }, (_, index) =>
    index % 2 ? `+const value${index} = ${index};` : `-const value${index} = ${index - 1};`,
  ),
].join("\n");

export const previewThread: ThreadDetail = {
  id: "1",
  preview: "Custom frontend skeleton",
  name: "Custom frontend skeleton",
  cwd: "/Users/ciaran/Projects/codex-custom",
  turns: [
    {
      id: "t1",
      status: "completed",
      completedAt: Date.now() / 1000 - 3600,
      durationMs: 24_000,
      items: [
        {
          type: "userMessage",
          id: "i1",
          content: [
            { type: "text", text: "Add a debounce helper to `utils.ts` and show me how to use it." },
            { type: "image", url: previewPixel },
            { type: "localImage", path: "/Users/ciaran/Desktop/screenshot.png" },
            { type: "text", text: "[utils.ts](src/lib/utils.ts)" },
          ],
        },
        {
          type: "reasoning",
          id: "i2",
          summary: [
            "**Planning the helper** — a generic debounce with a trailing call fits the existing utility style.",
          ],
        },
        {
          type: "commandExecution",
          id: "i3",
          command: 'rg -n "export function" src/lib/utils.ts',
          status: "completed",
          exitCode: 0,
          durationMs: 84,
          aggregatedOutput:
            "3:export function clamp(value: number, min: number, max: number) {\n8:export function sleep(ms: number) {",
        },
        {
          // The item id is the callId the spawn was answered under, which is
          // what links the transcript row to its agent run.
          type: "dynamicToolCall",
          id: "call-1",
          tool: "pingex_spawn_agent",
          arguments: {
            name: "debounce audit",
            prompt: "Check every call site of the new debounce helper.",
          },
        },
        {
          type: "dynamicToolCall",
          id: "call-2",
          tool: "pingex_spawn_agent",
          arguments: {
            name: "test sweep",
            prompt: "Run the test suite and summarise failures.",
          },
        },
        {
          type: "webSearch",
          id: "i3b",
          query:
            "typescript debounce implementation trailing edge generic parameters best practices — including how to preserve `this` context and cancellation semantics for repeated rapid calls in event handlers",
        },
        {
          type: "reasoning",
          id: "i3c",
          summary: [
            "**Reviewing search results** — the trailing-edge pattern with `clearTimeout` is standard; no need for a leading-edge option here.",
          ],
        },
        {
          type: "fileChange",
          id: "i4",
          status: "completed",
          changes: [
            {
              path: "src/lib/utils.ts",
              kind: { type: "update" },
              diff: "@@ -8,3 +8,12 @@\n export function sleep(ms: number) {\n   return new Promise((resolve) => setTimeout(resolve, ms));\n }\n+\n+export function debounce<T extends (...args: never[]) => void>(fn: T, wait = 150) {\n+  let timer: ReturnType<typeof setTimeout>;\n+  return (...args: Parameters<T>) => {\n+    clearTimeout(timer);\n+    timer = setTimeout(() => fn(...args), wait);\n+  };\n+}",
            },
            {
              path: "src/lib/generated.ts",
              kind: { type: "add" },
              diff: previewLongDiff,
            },
          ],
        },
        {
          type: "agentMessage",
          id: "i5",
          text: 'Added `debounce` to `src/lib/utils.ts`. Use it like this:\n\n```ts\nimport { debounce } from "./lib/utils";\n\nconst onSearch = debounce((query: string) => {\n  fetchResults(query);\n}, 250);\n\ninput.addEventListener("input", (event) => {\n  onSearch((event.target as HTMLInputElement).value);\n});\n```\n\nThe trailing call always wins — earlier pending calls are cancelled.\n\n| Area | Added | Share | What it does |\n|---|---:|---:|---|\n| Edge worker | 4,103 | 37.5% | Edge worker, tests, release tooling, docs |\n| CLI payload tooling | 1,035 | 9.5% | Builds/deploys payload sets and checks readiness |\n| CSP updater | 567 | 5.2% | Rewrites the CSP when the manifest is modified |',
        },
      ],
    },
  ],
};

export const previewArchived: ArchivedThread[] = [
  {
    id: "archived-1",
    title: "Old research thread",
    cwd: "/Users/ciaran/Projects/codex-custom",
    updatedAt: Date.now() / 1000 - 86400 * 12,
  },
];

// --- History search and pagination (feature 11) ---

const now = () => Date.now() / 1000;

// A larger archived corpus so `Load more` and result counts are exercisable in
// the browser preview and Playwright without a live Codex home.
export const previewArchivedThreads: ThreadSummary[] = Array.from({ length: 14 }, (_, index) => ({
  id: `archived-${index + 1}`,
  cwd: index % 2 ? "/Users/ciaran/Projects/arctic-explorer" : "/Users/ciaran/Projects/codex-custom",
  title: index === 0 ? "Old research thread" : `Archived exploration ${index + 1}: search index and pagination notes`,
  updatedAt: now() - 86400 * (index + 2),
  status: "idle",
  pinned: false,
  parentThreadId: null,
  agentNickname: null,
  agentRole: null,
  projectId: null,
  sectionId: null,
  subagentCount: 0,
  hidden: false,
}));

// The search index mirrors active (from previewData) and archived threads.
const previewSearchIndex: ThreadSearchItem[] = [
  ...previewData.projects.flatMap((project) =>
    project.threads.map((thread) => ({
      id: thread.id,
      title: thread.title,
      preview: thread.title,
      cwd: project.path,
      updatedAt: thread.updatedAt,
      archived: false,
    })),
  ),
  ...previewArchivedThreads.map((thread) => ({
    id: thread.id,
    title: thread.title,
    preview: thread.title,
    cwd: thread.cwd,
    updatedAt: thread.updatedAt,
    archived: true,
  })),
];

const SEARCH_PAGE = 20;

export function previewSearchThreads(
  query: string,
  cursor: string | null,
  filter: ThreadSearchFilter | undefined,
  generation: number,
): ThreadSearchPage {
  const lowered = query.trim().toLowerCase();
  const archived = filter?.archived ?? false;
  const projectPath = filter?.projectPath ?? null;
  const matches = previewSearchIndex
    .filter((item) => item.archived === archived)
    .filter((item) => !projectPath || item.cwd === projectPath)
    .filter(
      (item) =>
        !lowered ||
        item.title.toLowerCase().includes(lowered) ||
        item.preview.toLowerCase().includes(lowered) ||
        item.cwd.toLowerCase().includes(lowered),
    )
    .sort((a, b) => b.updatedAt - a.updatedAt);
  const offset = Number.parseInt(cursor ?? "", 10) || 0;
  const items = matches.slice(offset, offset + SEARCH_PAGE);
  const nextOffset = offset + items.length;
  return {
    items,
    nextCursor: nextOffset < matches.length ? String(nextOffset) : null,
    total: matches.length,
    generation,
  };
}

export function previewThreadsPage(
  cursor: string | null,
  pageSize: number,
  archived: boolean,
  projectPath: string | null,
): ThreadsPage {
  const source = archived
    ? previewArchivedThreads
    : previewData.projects.flatMap((project) => project.threads.map((thread) => ({ ...thread, cwd: project.path })));
  const filtered = projectPath ? source.filter((thread) => thread.cwd === projectPath) : source;
  const offset = Number.parseInt(cursor ?? "", 10) || 0;
  const items = filtered.slice(offset, offset + pageSize);
  const nextOffset = offset + items.length;
  return {
    items,
    nextCursor: nextOffset < filtered.length ? String(nextOffset) : null,
  };
}

export const previewModels: Model[] = [
  {
    id: "gpt-5.2-codex",
    model: "gpt-5.2-codex",
    displayName: "GPT-5.2 Codex",
    description: "Best for day-to-day coding",
    hidden: false,
    supportedReasoningEfforts: [
      { reasoningEffort: "low", description: "Fastest" },
      { reasoningEffort: "medium", description: "Balanced" },
      { reasoningEffort: "high", description: "Most thorough" },
    ],
    defaultReasoningEffort: "medium",
    isDefault: true,
  },
  {
    id: "gpt-5.2",
    model: "gpt-5.2",
    displayName: "GPT-5.2",
    description: "General purpose",
    hidden: false,
    supportedReasoningEfforts: [
      { reasoningEffort: "medium", description: "Balanced" },
      { reasoningEffort: "high", description: "Most thorough" },
      { reasoningEffort: "xhigh", description: "Extra thorough" },
    ],
    defaultReasoningEffort: "medium",
    isDefault: false,
    upgrade: "gpt-5.2-codex",
    upgradeInfo: {
      model: "gpt-5.2-codex",
      upgradeCopy: "GPT-5.2 is retiring — switch to GPT-5.2 Codex.",
      retirementAt: 1767225600,
    },
  },
];

export const previewQueues = new Map<string, QueuedSubmission[]>();

export function previewQueue(threadId: string): QueuedSubmission[] {
  let queue = previewQueues.get(threadId);
  if (!queue) {
    queue = [];
    previewQueues.set(threadId, queue);
  }
  return queue;
}

export function previewThreadUsage(threadId: string): ThreadUsage {
  return {
    threadId,
    estimatedUsageCreditsMicros: 1_234_500,
    estimatedUsageUsdMicros: 2_469_000,
    groups: [
      {
        model: "gpt-5.2-codex",
        reasoningEffort: "medium",
        estimatedUsageCreditsMicros: 1_234_500,
        netNewInputTokens: 18_400,
        cachedInputTokens: 96_000,
        inputTokens: 114_400,
        outputTokens: 6_200,
        totalTokens: 120_600,
      },
    ],
  };
}

export const previewFiles: FileHit[] = [
  { path: "src/lib/utils.ts", fileName: "utils.ts", score: 40, isDir: false },
  { path: "src/lib/api.ts", fileName: "api.ts", score: 32, isDir: false },
  { path: "src/lib", fileName: "lib", score: 24, isDir: true },
  { path: "README.md", fileName: "README.md", score: 20, isDir: false },
  { path: "src/App.svelte", fileName: "App.svelte", score: 18, isDir: false },
];

// --- Project instructions, sources, and workspace search (browser preview) ---

/** In-memory source store keyed by project path, seeded from previewData. */
const previewSources = new Map<string, ProjectSource[]>(
  previewData.projects
    .filter((project) => project.sources?.length)
    .map((project) => [project.path, structuredClone(project.sources ?? [])]),
);

export function previewListSources(projectPath: string): ProjectSource[] {
  return structuredClone(previewSources.get(projectPath) ?? []);
}

export function previewSaveInstructions(projectPath: string, instructions: string): void {
  const project = previewData.projects.find((entry) => entry.path === projectPath);
  if (project) project.instructions = instructions;
}

export function previewAddSource(projectPath: string, sourcePath: string, kind: "folder" | "file"): ProjectSource[] {
  const list = previewSources.get(projectPath) ?? [];
  list.push({
    id: `src-preview-${nextPreviewId()}`,
    projectPath,
    sourcePath,
    kind,
    addedAt: Date.now() / 1000,
    status: "indexed",
    indexedAt: Date.now() / 1000,
    docCount: kind === "file" ? 1 : 12,
    error: null,
  });
  previewSources.set(projectPath, list);
  return structuredClone(list);
}

export function previewRemoveSource(id: string, projectPath: string): ProjectSource[] {
  const list = (previewSources.get(projectPath) ?? []).filter((source) => source.id !== id);
  previewSources.set(projectPath, list);
  return structuredClone(list);
}

export function previewReindexSource(id: string): void {
  for (const list of previewSources.values()) {
    const source = list.find((entry) => entry.id === id);
    if (source) {
      source.status = "indexed";
      source.indexedAt = Date.now() / 1000;
    }
  }
}

export function previewSearchWorkspace(
  projectPath: string,
  query: string,
  cursor: string | null,
  generation: number,
): WorkspaceSearchResults {
  const trimmed = query.trim().toLowerCase();
  const empty: WorkspaceSearchResults = {
    projectFiles: { items: [], nextCursor: null, hasMore: false },
    threads: { items: [], nextCursor: null, hasMore: false },
    messages: { items: [], nextCursor: null, hasMore: false },
    generation,
  };
  if (!trimmed || cursor) return empty;
  const project = previewData.projects.find((entry) => entry.path === projectPath);
  return {
    projectFiles: {
      items: previewFiles
        .filter((file) => file.path.toLowerCase().includes(trimmed))
        .map((file) => ({
          path: file.path,
          fileName: file.fileName,
          lineNumber: file.isDir ? null : 12,
          preview: file.isDir ? null : `// matched "${query}" in ${file.fileName}`,
          nameMatch: file.fileName.toLowerCase().includes(trimmed),
        })),
      nextCursor: null,
      hasMore: false,
    },
    threads: {
      items: (project?.threads ?? [])
        .filter((thread) => thread.title.toLowerCase().includes(trimmed))
        .map((thread) => ({ threadId: thread.id, title: thread.title, cwd: thread.cwd })),
      nextCursor: null,
      hasMore: false,
    },
    messages: { items: [], nextCursor: null, hasMore: false },
    generation,
  };
}

// Mirrors a real `account/rateLimits/read`: the weekly window arrives as
// `primary` with no `secondary`, plus a separate per-model bucket.
export const previewRateLimits: AccountRateLimits = {
  rateLimits: {
    limitId: "codex",
    limitName: null,
    planType: "pro",
    primary: {
      usedPercent: 34,
      windowDurationMins: 10_080,
      resetsAt: Math.floor(Date.now() / 1000) + 60 * 60 * 52,
    },
    secondary: null,
  },
  rateLimitsByLimitId: {
    codex_spark: {
      limitId: "codex_spark",
      limitName: "GPT-5.3-Codex-Spark",
      primary: {
        usedPercent: 12,
        windowDurationMins: 10_080,
        resetsAt: Math.floor(Date.now() / 1000) + 60 * 60 * 70,
      },
      secondary: null,
    },
  },
};

const nowSeconds = () => Math.floor(Date.now() / 1000);

// Browser preview boots straight into the app (needsPicker: false) so the
// existing Playwright flows are unchanged; the picker is reachable via the
// homepage "Switch home" affordance and covered by its own unit test.
export const previewLaunchState: LaunchState = {
  codexHome: "~/.codex-personal",
  homeKey: "~/.codex-personal",
  codexBinary: "codex",
  defaultHome: "~/.codex",
  explicit: true,
  needsPicker: false,
  recentHomes: [
    { path: "~/.codex-personal", lastUsed: nowSeconds() - 120, exists: true },
    { path: "~/.codex-work", lastUsed: nowSeconds() - 8600, exists: true },
    { path: "~/.codex-archive", lastUsed: nowSeconds() - 900000, exists: false },
  ],
  codexBinaryStatus: {
    binary: "codex",
    resolved: "/opt/homebrew/bin/codex",
    found: true,
    message: null,
  },
};

export const previewHomeOverview: HomeOverview = {
  codexHome: "~/.codex-personal",
  codexBinary: "codex",
  configExists: true,
  model: "gpt-5.6-luna",
  reasoningEffort: "xhigh",
  approvalPolicy: "on-request",
  sandboxMode: "workspace-write",
  mcpServers: [
    { name: "computer-use", command: "./SkyComputerUseClient" },
    { name: "node_repl", command: "/Applications/ChatGPT.app/Contents/Resources/cua_node/bin/node_repl" },
    { name: "openaiDeveloperDocs", command: null },
  ],
  skills: [{ name: "agents-sdk" }, { name: "cloudflare" }, { name: "web-perf" }, { name: "wrangler" }],
};

export const previewConfigSettings: ConfigSetting[] = [
  {
    key: "model",
    section: "agent",
    label: "Model",
    kind: "string",
    value: "gpt-5.6-luna",
    default: null,
    source: "config",
    options: [],
    restartRequired: false,
  },
  {
    key: "model_reasoning_effort",
    section: "agent",
    label: "Reasoning effort",
    kind: "enum",
    value: "xhigh",
    default: "medium",
    source: "config",
    options: ["minimal", "low", "medium", "high", "xhigh"],
    restartRequired: false,
  },
  {
    key: "approval_policy",
    section: "agent",
    label: "Approval policy",
    kind: "enum",
    value: "on-request",
    default: "on-request",
    source: "default",
    options: ["untrusted", "on-failure", "on-request", "never"],
    restartRequired: false,
  },
  {
    key: "sandbox_mode",
    section: "agent",
    label: "Sandbox mode",
    kind: "enum",
    value: "workspace-write",
    default: "read-only",
    source: "config",
    options: ["read-only", "workspace-write", "danger-full-access"],
    restartRequired: false,
  },
  {
    key: "model_reasoning_summary",
    section: "modelFeatures",
    label: "Reasoning summaries",
    kind: "enum",
    value: "auto",
    default: "auto",
    source: "default",
    options: ["auto", "concise", "detailed", "none"],
    restartRequired: false,
  },
  {
    key: "hide_agent_reasoning",
    section: "modelFeatures",
    label: "Hide reasoning stream",
    kind: "bool",
    value: "false",
    default: "false",
    source: "default",
    options: [],
    restartRequired: false,
  },
  {
    key: "file_opener",
    section: "coding",
    label: "File opener scheme",
    kind: "enum",
    value: "vscode",
    default: "vscode",
    source: "default",
    options: ["vscode", "vscode-insiders", "windsurf", "cursor", "none"],
    restartRequired: false,
  },
];

// Keyed by project path so the browser preview can serve per-repo Git data.
export const previewRepoInfo: Record<string, GitRepoInfo> = {
  "/Users/ciaran/Projects/codex-custom": {
    dir: "/Users/ciaran/Projects/codex-custom",
    isGitRepo: true,
    root: "/Users/ciaran/Projects/codex-custom",
    commonDir: "/Users/ciaran/Projects/codex-custom/.git",
    branch: "main",
    detached: false,
    upstream: "origin/main",
    ahead: 2,
    behind: 0,
    inProgress: null,
    error: null,
  },
};

const previewDefaultRepoInfo = (dir: string): GitRepoInfo => ({
  dir,
  isGitRepo: true,
  root: dir,
  commonDir: `${dir}/.git`,
  branch: "main",
  detached: false,
  upstream: null,
  ahead: 0,
  behind: 0,
  inProgress: null,
  error: null,
});

export function previewGitRepoInfo(dir: string): GitRepoInfo {
  return previewRepoInfo[dir] ?? previewDefaultRepoInfo(dir);
}

export const previewGitStatus: GitStatus = {
  branch: "main",
  detached: false,
  upstream: "origin/main",
  ahead: 2,
  behind: 0,
  counts: { staged: 1, unstaged: 3, untracked: 2, conflicted: 0 },
  files: [
    { path: "src/lib/services/git.ts", state: "staged", code: "M." },
    { path: "src/App.svelte", state: "unstaged", code: ".M" },
    { path: "src/lib/types.ts", state: "unstaged", code: ".M" },
    { path: "src/lib/panels/Worktrees.svelte", state: "unstaged", code: ".M" },
    { path: "scratch/notes.md", state: "untracked", code: "" },
    { path: "scratch/todo.md", state: "untracked", code: "" },
  ],
  truncated: false,
  refreshedAt: Date.now(),
};

export const previewWorktrees: WorktreeEntry[] = [
  {
    path: "/Users/ciaran/Projects/codex-custom",
    head: "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b",
    branch: "main",
    detached: false,
    bare: false,
    locked: false,
    lockReason: null,
    prunable: false,
    prunableReason: null,
    isMain: true,
    isCodexManaged: false,
    missingDir: false,
    branchCheckedOutElsewhere: false,
    upstream: "origin/main",
    ahead: 2,
    behind: 0,
    status: { staged: 1, unstaged: 3, untracked: 2, conflicted: 0 },
    state: null,
  },
  {
    path: "/Users/ciaran/.codex/worktrees/0357/search-ranking",
    head: "9f8e7d6c5b4a3f2e1d0c9b8a7f6e5d4c3b2a1f0e",
    branch: "search-ranking",
    detached: false,
    bare: false,
    locked: false,
    lockReason: null,
    prunable: false,
    prunableReason: null,
    isMain: false,
    isCodexManaged: true,
    missingDir: false,
    branchCheckedOutElsewhere: false,
    upstream: null,
    ahead: 5,
    behind: 1,
    status: { staged: 0, unstaged: 0, untracked: 0, conflicted: 0 },
    state: null,
  },
  {
    path: "/Users/ciaran/Projects/experiments/detached-review",
    head: "3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c",
    branch: null,
    detached: true,
    bare: false,
    locked: true,
    lockReason: "reviewing a release build",
    prunable: false,
    prunableReason: null,
    isMain: false,
    isCodexManaged: false,
    missingDir: false,
    branchCheckedOutElsewhere: false,
    upstream: null,
    ahead: 0,
    behind: 0,
    status: { staged: 0, unstaged: 1, untracked: 0, conflicted: 0 },
    state: "detached",
  },
  {
    path: "/Users/ciaran/Projects/experiments/gone",
    head: "0000000000000000000000000000000000000000",
    branch: "stale-feature",
    detached: false,
    bare: false,
    locked: false,
    lockReason: null,
    prunable: true,
    prunableReason: "gitdir file points to non-existent location",
    isMain: false,
    isCodexManaged: false,
    missingDir: true,
    branchCheckedOutElsewhere: false,
    upstream: null,
    ahead: 0,
    behind: 0,
    status: null,
    state: "missingDir",
  },
];

export const previewCommits: GitCommit[] = [
  {
    hash: "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b",
    shortHash: "1a2b3c4",
    subject: "feat: use and limits",
    author: "Ciaran Kelly",
    timestamp: Math.floor(Date.now() / 1000) - 3600,
  },
  {
    hash: "2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c",
    shortHash: "2b3c4d5",
    subject: "feat: compact and usage bar",
    author: "Ciaran Kelly",
    timestamp: Math.floor(Date.now() / 1000) - 7200,
  },
  {
    hash: "3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b1c2d",
    shortHash: "3c4d5e6",
    subject: "fix: better plan stuff",
    author: "Ciaran Kelly",
    timestamp: Math.floor(Date.now() / 1000) - 10800,
  },
];

export const previewBranches: GitBranch[] = [
  { name: "main", isRemote: false, isCurrent: true },
  { name: "origin/main", isRemote: true, isCurrent: false },
  { name: "feat/review-picker", isRemote: false, isCurrent: false },
  { name: "fix/thread-scroll", isRemote: false, isCurrent: false },
];

// --- Pull-request review preview data ---

export const previewProviderStatus: ProviderStatus = {
  installed: true,
  authenticated: true,
  message: null,
};

export const previewPrs: PrSummary[] = [
  {
    number: 128,
    title: "Add pull-request review view",
    author: "ciaran",
    state: "OPEN",
    isDraft: false,
    baseRef: "main",
    headRef: "feature/pr-review",
    updatedAt: "2026-07-22T14:30:00Z",
    url: "https://github.com/ciaran/codex-custom/pull/128",
  },
  {
    number: 126,
    title: "Native git worktree service",
    author: "octo-helper",
    state: "OPEN",
    isDraft: true,
    baseRef: "main",
    headRef: "feature/worktrees",
    updatedAt: "2026-07-21T09:10:00Z",
    url: "https://github.com/ciaran/codex-custom/pull/126",
  },
];

const previewReviewPatch = [
  "@@ -1,6 +1,9 @@",
  " import { readFileSync } from 'fs';",
  " ",
  "-export function load(path) {",
  "-  return readFileSync(path);",
  "+export function load(path: string): Buffer {",
  "+  if (!path) {",
  "+    throw new Error('path is required');",
  "+  }",
  "+  return readFileSync(path);",
  " }",
  " ",
  " export const VERSION = 1;",
].join("\n");

export const previewPrDetail: PrDetail = {
  summary: previewPrs[0],
  body: "This adds a three-pane review view: PR summary and files on the left, diff in the center, and review/comments on the right.\n\nCloses #99.",
  headSha: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0",
  commits: [
    { oid: "a1b2c3d4e5f6", shortOid: "a1b2c3d", headline: "Add review service and adapter", author: "ciaran" },
    { oid: "b2c3d4e5f6a7", shortOid: "b2c3d4e", headline: "Wire the three-pane view", author: "ciaran" },
  ],
  files: [
    {
      path: "src/lib/loader.ts",
      oldPath: null,
      status: "modified",
      additions: 5,
      deletions: 2,
      patch: previewReviewPatch,
      patchTruncated: false,
      hunks: [
        {
          header: "@@ -1,6 +1,9 @@",
          oldStart: 1,
          oldLines: 6,
          newStart: 1,
          newLines: 9,
          lines: [
            { kind: "context", content: "import { readFileSync } from 'fs';", oldLine: 1, newLine: 1 },
            { kind: "context", content: "", oldLine: 2, newLine: 2 },
            { kind: "del", content: "export function load(path) {", oldLine: 3, newLine: null },
            { kind: "del", content: "  return readFileSync(path);", oldLine: 4, newLine: null },
            { kind: "add", content: "export function load(path: string): Buffer {", oldLine: null, newLine: 3 },
            { kind: "add", content: "  if (!path) {", oldLine: null, newLine: 4 },
            { kind: "add", content: "    throw new Error('path is required');", oldLine: null, newLine: 5 },
            { kind: "add", content: "  }", oldLine: null, newLine: 6 },
            { kind: "add", content: "  return readFileSync(path);", oldLine: null, newLine: 7 },
            { kind: "context", content: "}", oldLine: 5, newLine: 8 },
          ],
        },
      ],
    },
    {
      path: "docs/review.md",
      oldPath: null,
      status: "added",
      additions: 3,
      deletions: 0,
      patch: "@@ -0,0 +1,3 @@\n+# Review\n+\n+How to review a pull request.",
      patchTruncated: false,
      hunks: [
        {
          header: "@@ -0,0 +1,3 @@",
          oldStart: 0,
          oldLines: 0,
          newStart: 1,
          newLines: 3,
          lines: [
            { kind: "add", content: "# Review", oldLine: null, newLine: 1 },
            { kind: "add", content: "", oldLine: null, newLine: 2 },
            { kind: "add", content: "How to review a pull request.", oldLine: null, newLine: 3 },
          ],
        },
      ],
    },
  ],
  comments: [
    {
      id: 5001,
      author: "octo-helper",
      body: "Nice, but should we validate the path is absolute too?",
      createdAt: "2026-07-22T13:00:00Z",
      path: "src/lib/loader.ts",
      line: 4,
      side: "RIGHT",
      threadId: "THREAD_A",
      isResolved: false,
    },
    {
      id: 5002,
      author: "ciaran",
      body: "Good call — I'll leave absolute-path handling to the caller for now.",
      createdAt: "2026-07-22T13:20:00Z",
      path: "src/lib/loader.ts",
      line: 4,
      side: "RIGHT",
      threadId: "THREAD_A",
      isResolved: false,
    },
    {
      id: 0,
      author: "reviewer-bot",
      body: "Overall this looks solid. One general note: please add a test for the empty-path case.",
      createdAt: "2026-07-22T13:45:00Z",
      path: null,
      line: null,
      side: null,
      threadId: null,
      isResolved: false,
    },
  ],
  checks: { total: 4, passing: 3, failing: 0, pending: 1 },
  filesTruncated: false,
};

// Fake paired devices so browser preview renders connection cards. One is
// freshly active (online), one was seen a few hours ago, one is only known
// locally (never reported by the relay this session).
export const previewConnections: RemoteConnection[] = [
  {
    clientId: "device-iphone",
    name: "Ciaran's iPhone",
    platform: "iOS",
    deviceModel: "iPhone 16 Pro",
    appVersion: "1.2024.30",
    pairedAt: Date.now() / 1000 - 86400 * 4,
    lastSeen: Date.now() / 1000 - 40,
    scope: "full",
    source: "protocol",
  },
  {
    clientId: "device-ipad",
    name: "Kitchen iPad",
    platform: "iPadOS",
    deviceModel: "iPad Air",
    appVersion: "1.2024.28",
    pairedAt: Date.now() / 1000 - 86400 * 20,
    lastSeen: Date.now() / 1000 - 60 * 60 * 5,
    scope: "full",
    source: "protocol",
  },
  {
    clientId: "device-android",
    name: "Pixel device",
    platform: "android",
    deviceModel: null,
    appVersion: null,
    pairedAt: Date.now() / 1000 - 86400 * 60,
    lastSeen: null,
    scope: null,
    source: "local",
  },
];

// Quick-chat window: a mutable holder so browser-preview shortcut edits stick
// across the settings recorder round-trip.
export const previewQuickShortcut = { value: "CmdOrCtrl+Shift+Space" };

/** One finished agent and one still working, so both card states render. */
export const previewAgentRuns: AgentRun[] = [
  {
    runId: "agt_1",
    parentThreadId: "1",
    parentTurnId: "t1",
    callId: "call-1",
    childThreadId: "agent-thread-1",
    name: "debounce audit",
    prompt: "Check every call site of the new debounce helper.",
    cwd: "/Users/ciaran/Projects/codex-custom",
    model: "gpt-5.2",
    reasoningEffort: "medium",
    status: "done",
    result: "Found 3 call sites; all pass a delay. No changes needed.",
    error: null,
    createdAt: Date.now() - 120_000,
    finishedAt: Date.now() - 90_000,
  },
  {
    runId: "agt_2",
    parentThreadId: "1",
    parentTurnId: "t1",
    callId: "call-2",
    childThreadId: "agent-thread-2",
    name: "test sweep",
    prompt: "Run the test suite and summarise failures.",
    cwd: "/Users/ciaran/Projects/codex-custom",
    model: "gpt-5.2",
    reasoningEffort: null,
    status: "running",
    result: null,
    error: null,
    createdAt: Date.now() - 30_000,
    finishedAt: null,
  },
];

export const previewAgentSettings: AgentSettings = {
  enabled: true,
  sandbox: "workspace-write",
  maxConcurrent: 4,
  timeoutSeconds: 900,
  sandboxOptions: ["read-only", "workspace-write"],
};

// Mutable so the browser preview reflects add/remove/enable actions.
export const previewIntegrations: IntegrationsList = {
  mcpServers: [
    {
      name: "github",
      transport: "stdio",
      command: "npx",
      args: ["-y", "@modelcontextprotocol/server-github"],
      url: null,
      envKeys: ["GITHUB_TOKEN"],
      bearerTokenEnvVar: null,
      enabled: true,
      scope: "global",
    },
    {
      name: "linear",
      transport: "http",
      command: null,
      args: [],
      url: "https://mcp.linear.app/sse",
      envKeys: [],
      bearerTokenEnvVar: "LINEAR_API_KEY",
      enabled: false,
      scope: "global",
    },
    {
      name: "filesystem",
      transport: "stdio",
      command: "uvx",
      args: ["mcp-server-filesystem"],
      url: null,
      envKeys: [],
      bearerTokenEnvVar: null,
      enabled: true,
      scope: "global",
    },
  ],
  skills: [
    {
      name: "code-reviewer",
      path: "~/.codex/skills/code-reviewer/SKILL.md",
      scope: "user",
      description: "Review a diff for correctness, security, and style. Use when asked to review changes.",
      enabled: true,
      displayName: null,
      shortDescription: null,
    },
    {
      name: "pdf-filler",
      path: "~/.codex/skills/pdf-filler/SKILL.md",
      scope: "user",
      description: "Fill in PDF forms from structured data.",
      enabled: false,
      displayName: null,
      shortDescription: null,
    },
    {
      name: "browser-use:browser",
      path: "~/.codex/plugins/cache/browser-use/skills/browser/SKILL.md",
      scope: "system",
      description: "Browser automation for the in-app browser. Navigate, click, type, and screenshot.",
      enabled: true,
      displayName: "Browser",
      shortDescription: "Open and control the in-app browser.",
    },
  ],
  plugins: [],
  pluginsSupported: false,
};

/**
 * Live server state for the browser preview, covering every branch the UI
 * renders: a healthy stdio server with tools, an HTTP server that needs an
 * OAuth sign-in, and one that failed to start.
 *
 * Mutable so `previewMcpOauthLogin` can flip a server to signed-in.
 */
const previewStatuses: McpServerStatus[] = [
  {
    name: "github",
    serverInfo: {
      name: "github-mcp",
      title: "GitHub",
      version: "1.4.0",
      description: "Issues, pull requests, and code search for repositories you can access.",
      websiteUrl: "https://github.com/github/github-mcp-server",
    },
    resources: [{ uri: "github://repos/me/pingex/README.md", name: "README", mimeType: "text/markdown" }],
    resourceTemplates: [
      { uriTemplate: "github://repos/{owner}/{repo}/issues/{number}", name: "issue", description: "A single issue." },
    ],
    tools: {
      create_issue: {
        name: "create_issue",
        title: "Create issue",
        description: "Open a new issue on a repository.",
        inputSchema: {
          type: "object",
          properties: {
            repo: { type: "string", description: "owner/name of the repository" },
            title: { type: "string", description: "Issue title" },
            body: { type: "string", description: "Markdown body" },
          },
          required: ["repo", "title"],
        },
      },
      search_code: {
        name: "search_code",
        description: "Search code across repositories you can access.",
        inputSchema: {
          type: "object",
          properties: { query: { type: "string", description: "GitHub code-search query" } },
          required: ["query"],
        },
      },
    },
    authStatus: "unsupported",
  },
  {
    name: "linear",
    serverInfo: { name: "linear", title: "Linear", version: "2.0.1" },
    tools: {},
    authStatus: "notLoggedIn",
  },
  {
    name: "filesystem",
    serverInfo: null,
    tools: {},
    authStatus: "unsupported",
    error: "Server exited before completing the handshake",
  },
];

const PREVIEW_SKILL_BODY: Record<string, string> = {
  "code-reviewer": `---
name: code-reviewer
description: Review a diff for correctness, security, and style.
---

## Instructions

1. Read the whole diff before commenting.
2. Call out correctness bugs first, then security, then style.
3. Cite \`file:line\` for every finding.
`,
};

export function previewReadSkill(path: string): string {
  const name = path.split("/").filter(Boolean).at(-2) ?? "";
  return PREVIEW_SKILL_BODY[name] ?? `---\nname: ${name}\ndescription: (preview)\n---\n\n## Instructions\n`;
}

export function previewCreateSkill(input: {
  name: string;
  description: string;
  body?: string | null;
}): IntegrationsList {
  if (previewIntegrations.skills.some((skill) => skill.name === input.name)) {
    throw new Error(`A skill named ${input.name} already exists.`);
  }
  const path = `~/.codex/skills/${input.name}/SKILL.md`;
  PREVIEW_SKILL_BODY[input.name] = `---\nname: ${input.name}\ndescription: ${input.description}\n---\n\n${
    input.body?.trim() || "## Instructions\n"
  }\n`;
  previewIntegrations.skills.push({
    name: input.name,
    path,
    scope: "user",
    description: input.description,
    enabled: true,
    displayName: null,
    shortDescription: null,
  });
  previewIntegrations.skills.sort((a, b) => a.name.localeCompare(b.name));
  return structuredClone(previewIntegrations);
}

export function previewDeleteSkill(path: string): IntegrationsList {
  previewIntegrations.skills = previewIntegrations.skills.filter((skill) => skill.path !== path);
  return structuredClone(previewIntegrations);
}

export function previewMcpServerStatus(): { data: McpServerStatus[] } {
  return structuredClone({ data: previewStatuses });
}

/** Pretend the OAuth round-trip succeeded, so the UI can show the signed-in state. */
export function previewMcpOauthLogin(name: string): void {
  const status = previewStatuses.find((entry) => entry.name === name);
  if (!status) return;
  status.authStatus = "oAuth";
  status.tools = {
    create_issue: {
      name: "create_issue",
      description: "Create a Linear issue.",
      inputSchema: {
        type: "object",
        properties: { team: { type: "string" }, title: { type: "string" } },
        required: ["title"],
      },
    },
  };
}

/** Mirror of the native `save_mcp_server` command for the browser preview. */
export function previewSaveMcpServer(input: {
  previousName?: string | null;
  name: string;
  command?: string | null;
  args: string[];
  envKeys: string[];
  url?: string | null;
  bearerTokenEnvVar?: string | null;
}): void {
  const previous = input.previousName
    ? previewIntegrations.mcpServers.find((server) => server.name === input.previousName)
    : undefined;
  const stdio = Boolean(input.command?.trim());
  const summary: McpServerSummary = {
    name: input.name,
    transport: stdio ? "stdio" : "http",
    command: stdio ? (input.command?.trim() ?? null) : null,
    args: stdio ? input.args : [],
    url: stdio ? null : (input.url?.trim() ?? null),
    envKeys: stdio ? input.envKeys : [],
    bearerTokenEnvVar: stdio ? null : input.bearerTokenEnvVar?.trim() || null,
    enabled: previous?.enabled ?? true,
    scope: "global",
  };
  previewIntegrations.mcpServers = previewIntegrations.mcpServers.filter(
    (server) => server.name !== input.name && server.name !== input.previousName,
  );
  previewIntegrations.mcpServers.push(summary);
  previewIntegrations.mcpServers.sort((a, b) => a.name.localeCompare(b.name));
}

export const previewRuntimeSettings: RuntimeSettings = {
  codexHome: "~/.codex-personal",
  codexBinary: "codex",
  overrideCodexHome: null,
  overrideCodexBinary: null,
  overrideClaudeBinary: null,
  overrideClaudeConfigDir: null,
  settingsPath: "~/Library/Application Support/pingex/settings.json",
  restartRequired: false,
};

/** A short sample exchange so the message log has something to render in the
 * browser preview, where there is no app-server to listen to. */
export const previewWireLog: WireMessage[] = [
  {
    seq: 0,
    at: 1_753_700_000_000,
    direction: "out",
    kind: "request",
    method: "thread/sendMessage",
    id: 12,
    threadId: "thread-1",
    payload: { threadId: "thread-1", input: [{ type: "text", text: "Add a message log" }] },
    truncated: false,
  },
  {
    seq: 1,
    at: 1_753_700_000_120,
    direction: "in",
    kind: "notification",
    method: "turn/started",
    id: null,
    threadId: "thread-1",
    payload: { threadId: "thread-1", turnId: "turn-1" },
    truncated: false,
  },
  {
    seq: 2,
    at: 1_753_700_001_400,
    direction: "in",
    kind: "response",
    method: null,
    id: 12,
    threadId: null,
    payload: { turnId: "turn-1" },
    truncated: false,
  },
];

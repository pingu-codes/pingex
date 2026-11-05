// The browser preview's stand-in for the Rust backend. `services/homeRouting`
// dispatches every generated command here when the page is not running under
// Tauri; a command missing from the table resolves `undefined`. Each fake
// takes the same arguments the generated command does and returns the same
// wire shape, so the wrappers in `api.ts` run unchanged on top of it.
// Loaded with a dynamic `import()`, so none of this ships in the app bundle.

import { NATIVE_HOST, sameHost } from "$lib/app/host";
import type { commands } from "$lib/bindings";
import { deleteFromLayout, emptyLayout, nextOrdinal, placeInLayout, resetLayoutOrder } from "$lib/layout/sidebarTree";
import type {
  BinaryStatus,
  BootstrapData,
  IntegrationsList,
  Project,
  QueuedSubmission,
  ReviewDraft,
  SidebarLayout,
  ThreadGoal,
  ThreadSummary,
  UserInputPart,
} from "$lib/types";
import { basename } from "$lib/utils/path";
import { previewStageBytes, previewStageFromPath } from "./attachments";
import {
  deskDiscardPendingReview,
  deskInbox,
  deskMarkActivitySeen,
  deskMutePr,
  deskPendingReview,
  deskPrDetail,
  deskSavePendingReview,
  deskSetViewed,
  deskStacks,
  deskSubmitPendingReview,
  deskViewedFiles,
} from "./desk";
import { worktreeEntry } from "./factories";
import {
  nextPreviewId,
  previewAddSource,
  previewAddWorktreeProject,
  previewBranches,
  previewCommits,
  previewConfigSettings,
  previewConnections,
  previewContextBreakdown,
  previewCreateSkill,
  previewData,
  previewDeleteSkill,
  previewFilePreview,
  previewFiles,
  previewGitContext,
  previewGitRepoInfo,
  previewGitStatus,
  previewHomeOverview,
  previewIntegrations,
  previewLaunchState,
  previewListSources,
  previewMcpOauthLogin,
  previewMcpServerStatus,
  previewModels,
  previewPrDetail,
  previewPromptPartsText,
  previewProviderStatus,
  previewPrs,
  previewQueue,
  previewQuickShortcut,
  previewRateLimits,
  previewReadSkill,
  previewReindexSource,
  previewRemoveSource,
  previewRuntimeSettings,
  previewSaveInstructions,
  previewSaveMcpServer,
  previewSearchThreads,
  previewSearchWorkspace,
  previewSort,
  previewThread,
  previewThreadsPage,
  previewThreadUsage,
  previewUnarchive,
  previewUsageBreakdown,
  previewWireLog,
  previewWorktrees,
} from "./fixtures";
import {
  previewDeleteMemory,
  previewListMemories,
  previewReadMemory,
  previewSetMemoryToggle,
  previewWriteMemory,
} from "./memories";
import { repositoryCommands } from "./repositories";
import { previewCompact, previewInterrupt, previewStreamTurn } from "./stream";

type Commands = typeof commands;

/** A fake for each command the preview implements, typed like the generated
 *  command but free to answer synchronously or with nothing. */
export type PreviewCommands = {
  [K in keyof Commands]?: (
    ...args: Parameters<Commands[K]>
  ) => Awaited<ReturnType<Commands[K]>> | ReturnType<Commands[K]> | void | Promise<void>;
};

// --- Shared edits to the in-memory fixtures ---------------------------------

/** Apply `edit` to every listed thread with this id, then re-sort. */
function editThread(threadId: string, edit: (thread: ThreadSummary) => void): BootstrapData {
  for (const project of previewData.projects) {
    const thread = project.threads.find((thread) => thread.id === threadId);
    if (thread) edit(thread);
  }
  return previewSort();
}

/** Apply `edit` to the project at `path`, then re-sort. */
function editProject(path: string, edit: (project: Project) => void): BootstrapData {
  const project = previewData.projects.find((project) => project.path === path);
  if (project) edit(project);
  return previewSort();
}

function removeThread(threadId: string): BootstrapData {
  for (const project of previewData.projects) {
    project.threads = project.threads.filter((thread) => thread.id !== threadId);
  }
  return previewSort();
}

function layout(): SidebarLayout {
  previewData.sidebarLayout ??= emptyLayout();
  return previewData.sidebarLayout;
}

function dropConnection(clientId: string): void {
  const index = previewConnections.findIndex((entry) => entry.clientId === clientId);
  if (index >= 0) previewConnections.splice(index, 1);
}

const claudeStatus = {
  available: true,
  path: "/usr/local/bin/claude",
  version: "2.1.251",
  configDir: "~/.claude",
  loggedIn: true,
  protocolFloor: "1.0.59",
  message: null,
};

const previewTurn = () => ({ id: `preview-turn-${nextPreviewId()}`, status: "inProgress", items: [] });

/** A Codex CLI the preview pretends to find: bare `codex` or any `…/codex`. */
function probeCodexBinary(path: string | null): BinaryStatus {
  const binary = path?.trim() || previewLaunchState.codexBinaryStatus.binary;
  const found = binary === "codex" || binary.endsWith("/codex");
  return {
    binary,
    resolved: found ? (binary.includes("/") ? binary : "/opt/homebrew/bin/codex") : null,
    found,
    message: found ? null : `No executable Codex CLI at ${binary}.`,
  };
}

// Git actions edit the shared status fixture in place so the browser build
// shows stage/unstage/discard/commit working.
function recountGitStatus() {
  const counts = { staged: 0, unstaged: 0, untracked: 0, conflicted: 0 };
  for (const file of previewGitStatus.files) {
    if (file.state === "untracked") counts.untracked += 1;
    else if (file.state === "conflicted") counts.conflicted += 1;
    else {
      if (file.code[0] !== ".") counts.staged += 1;
      if (file.code[1] !== ".") counts.unstaged += 1;
    }
  }
  previewGitStatus.counts = counts;
}

function checkoutBranch(name: string, force: boolean | null): void {
  const dirty =
    previewGitStatus.counts.staged + previewGitStatus.counts.unstaged + previewGitStatus.counts.untracked > 0;
  if (dirty && !force)
    throw new Error(
      "dirtyTree: This checkout has uncommitted changes. Commit or discard them, or switch anyway to carry them over.",
    );
  previewGitStatus.branch = name;
  previewGitStatus.upstream = null;
  previewGitStatus.ahead = 0;
  previewGitStatus.behind = 0;
  for (const branch of previewBranches) branch.isCurrent = branch.name === name;
}

function previewPatch(path: string, hunk: string) {
  const patch = `diff --git a/${path} b/${path}\n--- a/${path}\n+++ b/${path}\n${hunk}`;
  return { path, patch, truncated: false, binary: false, bytes: patch.length };
}

const draftKey = (project: string) => `pingex-draft:${project}`;
const reviewDrafts = new Map<string, ReviewDraft>();
const reviewDraftKey = (provider: string, repo: string, prNumber: number) => `${provider}:${repo}:${prNumber}`;

const projectOverrides = new Map<string, Record<string, boolean>>();

function integrationView(projectPath?: string | null): IntegrationsList {
  const next = structuredClone(previewIntegrations);
  const overrides = projectPath ? (projectOverrides.get(projectPath) ?? {}) : {};
  for (const [kind, rows] of [
    ["mcp", next.mcpServers],
    ["plugin", next.plugins],
  ] as const) {
    for (const row of rows) {
      const id = "id" in row ? row.id : row.name;
      const key = `${kind}:${id}`;
      const inheritedEnabled = row.enabled;
      const overrideEnabled = overrides[key] ?? null;
      next.settings[key] = { inheritedEnabled, overrideEnabled, pluginId: null };
      row.enabled = overrideEnabled ?? inheritedEnabled;
    }
  }
  for (const skill of next.skills) {
    const owner = next.settings[`skill:${skill.path}`]?.pluginId;
    if (owner && next.plugins.some((plugin) => plugin.id === owner && !plugin.enabled)) skill.enabled = false;
  }
  return next;
}

// The accelerator the fixture starts with; clearing the field restores it.
const defaultQuickShortcut = previewQuickShortcut.value;

export const previewCommands: PreviewCommands = {
  // --- Projects, threads and the sidebar ---
  bootstrap: () => previewData,
  addWorktreeProject: (path) => previewAddWorktreeProject(path),
  createWorkspace: () => previewSort(),
  updateWorkspace: () => previewSort(),
  moveThreadToWorkspace: () => previewSort(),
  createThreadSection(name, color) {
    previewData.sections = [...(previewData.sections ?? []), { id: `section-${Date.now()}`, name, icon: null, color }];
    return previewSort();
  },
  updateThreadSection(sectionId, name, color) {
    const section = previewData.sections?.find((section) => section.id === sectionId);
    if (section) Object.assign(section, { name, color });
    return previewSort();
  },
  deleteThreadSection(sectionId) {
    previewData.sections = (previewData.sections ?? []).filter((section) => section.id !== sectionId);
    for (const project of previewData.projects) {
      for (const thread of project.threads) if (thread.sectionId === sectionId) thread.sectionId = null;
    }
    return previewSort();
  },
  moveThreadToSection: (threadId, sectionId) =>
    editThread(threadId, (thread) => {
      thread.sectionId = sectionId;
    }),
  renameProject: (path, name) =>
    editProject(path, (project) => {
      if (name.trim()) project.name = name.trim();
    }),
  renameThread: (threadId, name) =>
    editThread(threadId, (thread) => {
      if (name.trim()) thread.title = name.trim();
    }),
  autoNameThread: () => null,
  setProjectPinned: (path, pinned) =>
    editProject(path, (project) => {
      project.pinned = pinned;
    }),
  setProjectArchived: (path, archived) =>
    editProject(path, (project) => {
      project.archived = archived;
    }),
  setProjectExpanded(path, expanded) {
    const project = previewData.projects.find((project) => project.path === path);
    if (project) project.expanded = expanded;
  },
  setThreadPinned: (threadId, pinned) =>
    editThread(threadId, (thread) => {
      thread.pinned = pinned;
    }),
  setThreadsHidden(threadIds, hidden) {
    for (const project of previewData.projects) {
      for (const thread of project.threads) if (threadIds.includes(thread.id)) thread.hidden = hidden;
    }
    return previewSort();
  },
  resetSidebarOrder(scope) {
    previewData.sidebarLayout = resetLayoutOrder(layout(), scope);
    return previewSort();
  },
  applySessionFocus(hide, collapseProjects, collapseFolders) {
    for (const project of previewData.projects) {
      if (collapseProjects.includes(project.path)) project.expanded = false;
      for (const thread of project.threads) if (hide.includes(thread.id)) thread.hidden = true;
    }
    for (const folder of previewData.sidebarLayout?.folders ?? []) {
      if (collapseFolders.includes(folder.id)) folder.expanded = false;
    }
    return previewSort();
  },
  archiveThread: removeThread,
  deleteThread: removeThread,
  unarchiveThread(threadId) {
    previewUnarchive(threadId);
    return previewSort();
  },
  removeProject(path) {
    previewData.projects = previewData.projects.filter((project) => project.path !== path);
    return previewSort();
  },
  createSidebarFolder(scope, parentId, name) {
    const folders = layout();
    folders.folders.push({
      id: `folder-${Date.now()}-${folders.folders.length}`,
      scope,
      parentId,
      name,
      expanded: true,
      ordinal: nextOrdinal(folders, scope, parentId),
    });
    return previewSort();
  },
  renameSidebarFolder(id, name) {
    const folder = layout().folders.find((folder) => folder.id === id);
    if (folder) folder.name = name;
    return previewSort();
  },
  deleteSidebarFolder(id) {
    previewData.sidebarLayout = deleteFromLayout(layout(), id);
    return previewSort();
  },
  setSidebarFolderExpanded(id, expanded) {
    const folder = layout().folders.find((folder) => folder.id === id);
    if (folder) folder.expanded = expanded;
  },
  placeSidebarItem(scope, item, parentId, siblings) {
    previewData.sidebarLayout = placeInLayout(layout(), scope, item, parentId, siblings);
    return previewSort();
  },
  openExternalUrl(url) {
    window.open(url, "_blank", "noopener,noreferrer");
  },

  // --- Threads and turns ---
  readThread: (threadId) => ({ ...previewThread, id: threadId }),
  listSubagents(threadId) {
    const byId = new Map(previewData.subagents.map((thread) => [thread.id, thread]));
    const isDescendant = (thread: ThreadSummary) => {
      let parent = thread.parentThreadId ?? null;
      while (parent) {
        if (parent === threadId) return true;
        parent = byId.get(parent)?.parentThreadId ?? null;
      }
      return false;
    };
    return previewData.subagents.filter(isDescendant).map((thread, index) => ({
      id: thread.id,
      parentThreadId: thread.parentThreadId ?? threadId,
      title: thread.title,
      cwd: thread.cwd,
      status: thread.status,
      agentNickname: thread.agentNickname ?? null,
      agentRole: thread.agentRole ?? null,
      model: index === 0 ? "gpt-5.6-terra" : "gpt-5.6-sol",
      reasoningEffort: index === 0 ? "high" : "xhigh",
    }));
  },
  startThread: (cwd, _workspaceId, harness) => ({
    id: `preview-${nextPreviewId()}`,
    cwd,
    harness: harness ?? null,
  }),
  listHarnessModels: () => ({ data: previewModels }),
  listModels: () => ({ data: previewModels }),
  readHomeStatus: () => ({ ...claudeStatus }),
  startTurn(threadId, input) {
    const turn = previewTurn();
    const text = (input as UserInputPart[]).find((part) => part.type === "text")?.text ?? "";
    previewStreamTurn(threadId, turn.id, text);
    return turn;
  },
  interruptTurn(threadId, turnId) {
    previewInterrupt(threadId, turnId);
  },
  threadsWithUnansweredQuestions: () => [],
  threadsWithActiveTurns: () => [],
  compactThread(threadId) {
    previewCompact(threadId);
  },
  startReview: () => previewTurn(),
  threadGoalSet: (threadId, objective, status, tokenBudget): ThreadGoal => ({
    threadId,
    objective: objective ?? "",
    status: (status as ThreadGoal["status"] | null) ?? "active",
    tokenBudget,
    tokensUsed: 0,
    timeUsedSeconds: 0,
  }),
  threadGoalGet: () => null,
  readAccountRateLimits: () => previewRateLimits,
  forkThread: (_threadId, _beforeTurnId, _lastTurnId, cwd) => ({
    id: `preview-fork-${nextPreviewId()}`,
    cwd: cwd ?? undefined,
  }),
  revertThread: (threadId) => ({ thread: { ...previewThread, id: threadId, turns: [] } }),
  readThreadUsage: (threadId) => ({ threadUsage: previewThreadUsage(threadId) }),
  readUsageBreakdown: (scope, since) => previewUsageBreakdown(scope, since),
  readContextBreakdown: (threadId) => previewContextBreakdown(threadId),
  readPromptPartsText: (threadId) => previewPromptPartsText(threadId),
  updateTurnSettings: () => ({ status: "applied" }),

  // --- The server-side queue ---
  queueList: (threadId) => ({ data: [...previewQueue(threadId)], nextCursor: null }),
  queueAdd(threadId, input, clientUserMessageId) {
    const queuedSubmission: QueuedSubmission = {
      id: `preview-queued-${nextPreviewId()}`,
      input: input as UserInputPart[],
      clientUserMessageId,
    };
    previewQueue(threadId).push(queuedSubmission);
    return { queuedSubmission };
  },
  queueUpdate(threadId, queuedSubmissionId, input) {
    const entry = previewQueue(threadId).find((item) => item.id === queuedSubmissionId);
    if (entry) entry.input = input as UserInputPart[];
    return {
      queuedSubmission: entry ?? { id: queuedSubmissionId, input: input as UserInputPart[], clientUserMessageId: "" },
    };
  },
  queueDelete(threadId, queuedSubmissionId) {
    const queue = previewQueue(threadId);
    const index = queue.findIndex((item) => item.id === queuedSubmissionId);
    if (index >= 0) queue.splice(index, 1);
    return { deleted: index >= 0 };
  },
  queueReorder(threadId, queuedSubmissionIds) {
    previewQueue(threadId).sort((a, b) => queuedSubmissionIds.indexOf(a.id) - queuedSubmissionIds.indexOf(b.id));
  },

  // --- Side questions and branches ---
  sideQuestionMode: () => "fork",
  addSideQuestion(parentThreadId, sideThreadId, title, inheritedTurns) {
    previewData.sideQuestions.unshift({
      parentThreadId,
      sideThreadId,
      title,
      createdAt: Date.now() / 1000,
      inheritedTurns,
    });
    return previewSort();
  },
  addThreadBranch(parentThreadId, threadId, replacedTurnId, inheritedTurns) {
    previewData.threadBranches = [
      ...(previewData.threadBranches ?? []),
      {
        threadId,
        parentThreadId,
        groupTurnId:
          previewData.threadBranches?.find((b) => b.editTurnId === replacedTurnId)?.groupTurnId ?? replacedTurnId,
        replacedTurnId,
        inheritedTurns,
        editTurnId: null,
        createdAt: Date.now() / 1000,
        updatedAt: null,
      },
    ];
    return previewSort();
  },
  setThreadBranchEditTurn(threadId, editTurnId) {
    const branch = previewData.threadBranches?.find((b) => b.threadId === threadId);
    if (branch) branch.editTurnId = editTurnId;
  },
  removeSideQuestion(sideThreadId) {
    previewData.sideQuestions = previewData.sideQuestions.filter((entry) => entry.sideThreadId !== sideThreadId);
    return previewSort();
  },

  // --- Project files, instructions, sources and workspace search ---
  searchProjectFiles(_root, query, limit) {
    const lowered = query.toLowerCase();
    return previewFiles.filter((file) => file.path.toLowerCase().includes(lowered)).slice(0, limit ?? undefined);
  },
  listProjectFiles: () =>
    previewFiles
      .filter((file) => !file.isDir)
      .map((file) => file.path)
      .sort(),
  saveProjectInstructions(projectPath, instructions) {
    previewSaveInstructions(projectPath, instructions);
  },
  listProjectSources: (projectPath) => previewListSources(projectPath),
  addProjectSource: (projectPath, sourcePath, kind) =>
    previewAddSource(projectPath, sourcePath, kind as "folder" | "file"),
  removeProjectSource: (id, projectPath) => previewRemoveSource(id, projectPath),
  reindexSource(id) {
    previewReindexSource(id);
  },
  searchWorkspace: (projectPath, query, cursor, generation) =>
    previewSearchWorkspace(projectPath, query, cursor, generation ?? 0),
  stageAttachment: (sourcePath) => previewStageFromPath(sourcePath),
  stageClipboardImage: (filename, mime, bytes) => previewStageBytes(filename ?? "", mime ?? "", bytes),

  // Message drafts fall back to localStorage. `content` is the serialized
  // composer parts (JSON).
  saveDraft(project, content) {
    localStorage.setItem(draftKey(project), content);
  },
  loadDraft: (project) => localStorage.getItem(draftKey(project)),
  deleteDraft(project) {
    localStorage.removeItem(draftKey(project));
  },

  // --- Settings and Homes ---
  readRuntimeSettings: () => ({ ...previewRuntimeSettings }),
  readCodexServerInfo: () => ({ userAgent: "pingex/0.151.0 (preview; arm64) (pingex; 0.0.0)", platformOs: "preview" }),
  updateRuntimeSettings(codexHome, codexBinary, claudeBinary, claudeConfigDir, codexHost, claudeHost) {
    previewRuntimeSettings.overrideCodexHome = codexHome;
    previewRuntimeSettings.overrideCodexBinary = codexBinary;
    previewRuntimeSettings.overrideClaudeBinary = claudeBinary;
    previewRuntimeSettings.overrideClaudeConfigDir = claudeConfigDir;
    previewRuntimeSettings.overrideCodexHost = codexHost;
    previewRuntimeSettings.overrideClaudeHost = claudeHost;
    previewRuntimeSettings.restartRequired = Boolean(codexHome || codexBinary || codexHost);
    return { ...previewRuntimeSettings };
  },
  listWslDistros: () => [],
  readLaunchState: () => structuredClone(previewLaunchState),
  selectCodexHome(path, host) {
    const settled = host ?? NATIVE_HOST;
    previewLaunchState.codexHome = path;
    previewLaunchState.host = settled;
    previewLaunchState.needsPicker = false;
    previewLaunchState.recentHomes = [
      { path, lastUsed: Math.floor(Date.now() / 1000), exists: true, host: settled },
      ...previewLaunchState.recentHomes.filter((home) => home.path !== path || !sameHost(home.host, settled)),
    ];
    return structuredClone(previewLaunchState);
  },
  setCodexBinary(path) {
    const status = probeCodexBinary(path);
    if (!status.found) throw new Error(status.message ?? "Codex CLI not found");
    previewLaunchState.codexBinary = status.binary;
    previewLaunchState.codexBinaryStatus = status;
    return structuredClone(previewLaunchState);
  },
  removeRecentHome(path, host) {
    previewLaunchState.recentHomes = previewLaunchState.recentHomes.filter(
      (home) => home.path !== path || !sameHost(home.host, host),
    );
    return structuredClone(previewLaunchState);
  },
  readConfigSettings: () => structuredClone(previewConfigSettings),
  writeConfigSetting(key, value, unset) {
    const setting = previewConfigSettings.find((entry) => entry.key === key);
    if (setting) {
      setting.value = unset ? setting.default : value;
      setting.source = unset ? "default" : "config";
    }
    return structuredClone(previewConfigSettings);
  },
  readHomeOverview: () => structuredClone(previewHomeOverview),

  // --- Git ---
  gitRepoInfo: (dir) => previewGitRepoInfo(dir),
  gitStatus: () => ({ ...previewGitStatus, refreshedAt: Date.now() }),
  gitContext: (dir) => previewGitContext(dir),
  gitStage(_dir, paths) {
    for (const file of previewGitStatus.files) {
      if (!paths.includes(file.path)) continue;
      file.state = "staged";
      file.code = file.code ? `${file.code[1] === "." ? "M" : file.code[1]}.` : "A.";
    }
    recountGitStatus();
  },
  gitUnstage(_dir, paths) {
    for (const file of previewGitStatus.files) {
      if (!paths.includes(file.path) || file.state !== "staged") continue;
      if (file.code[0] === "A") {
        file.state = "untracked";
        file.code = "";
      } else {
        file.state = "unstaged";
        file.code = ".M";
      }
    }
    recountGitStatus();
  },
  gitDiscard(_dir, paths, untrackedPaths) {
    const gone = new Set([...paths, ...untrackedPaths]);
    previewGitStatus.files = previewGitStatus.files.filter((file) => !gone.has(file.path));
    recountGitStatus();
  },
  gitCommit(_dir, message) {
    previewGitStatus.files = previewGitStatus.files.filter((file) => file.code[0] === "." || file.state !== "staged");
    recountGitStatus();
    previewGitStatus.ahead += 1;
    return {
      hash: "9f8e7d6c5b4a39281706f5e4d3c2b1a0f9e8d7c6",
      shortHash: "9f8e7d6",
      subject: message.split("\n")[0],
      authorName: "Ciaran",
      authorEmail: "ciaran@example.com",
      hookOutput: "pre-commit: lint ok",
    };
  },
  gitFetch: () => ({
    operation: "fetch",
    summary: "Fetched",
    upstream: previewGitStatus.upstream,
    ahead: previewGitStatus.ahead,
    behind: previewGitStatus.behind,
  }),
  gitPull() {
    previewGitStatus.behind = 0;
    return {
      operation: "pull",
      summary: "Already up to date",
      upstream: previewGitStatus.upstream,
      ahead: previewGitStatus.ahead,
      behind: 0,
    };
  },
  gitPush(_dir, setUpstream) {
    if (!previewGitStatus.upstream && !setUpstream)
      throw new Error("noUpstream: This branch has no upstream. Publish it to create one.");
    if (setUpstream) previewGitStatus.upstream = `origin/${previewGitStatus.branch ?? "main"}`;
    previewGitStatus.ahead = 0;
    return {
      operation: "push",
      summary: `Pushed to ${previewGitStatus.upstream}`,
      upstream: previewGitStatus.upstream,
      ahead: 0,
      behind: 0,
    };
  },
  gitCheckoutBranch(_dir, name, force) {
    checkoutBranch(name, force);
  },
  gitCreateBranch(_dir, name, _base, checkout) {
    previewBranches.unshift({ name, isRemote: false, isCurrent: false });
    if (checkout) checkoutBranch(name, true);
  },
  gitStagedFileDiff: (_dir, path) => previewPatch(path, "@@ -1,2 +1,2 @@\n line\n-before\n+staged change\n"),
  gitFileDiff: (_dir, _base, path) => previewPatch(path, "@@ -1,2 +1,3 @@\n line\n-old\n+new\n+added\n"),
  gitChangesSummary: () => ({
    base: "HEAD",
    baseBranch: null,
    files: [
      { path: "src/lib/example.ts", oldPath: null, status: "modified", additions: 12, deletions: 3, binary: false },
      { path: "README.md", oldPath: null, status: "added", additions: 40, deletions: 0, binary: false },
    ],
    truncated: false,
    totalFiles: 2,
    additions: 52,
    deletions: 3,
  }),
  gitRecentCommits: () => structuredClone(previewCommits),
  gitBranches: () => structuredClone(previewBranches),

  // --- Worktrees ---
  gitWorktrees: () => structuredClone(previewWorktrees),
  gitWorktreeHandoffPreflight: () => ({
    branch: "codex/tmp-preview",
    worktreeDirty: false,
    targetDirty: false,
    blocker: null,
  }),
  gitWorktreeHandoff: (_worktreePath, _targetDir, _commitUncommitted, branchName) => branchName || "codex/tmp-preview",
  gitWorktreeAdd(_repoDir, { path, branch }) {
    previewWorktrees.push(worktreeEntry({ path, head: null, branch: branch.name }));
  },
  gitWorktreeAddTemp(repoDir) {
    const suffix = `${Date.now().toString(36)}-${crypto.randomUUID().slice(0, 6)}`;
    const path = `${previewLaunchState.codexHome}/worktrees-tmp/${basename(repoDir)}/${suffix}`;
    previewWorktrees.push(
      worktreeEntry({ path, head: null, branch: `codex/tmp-${suffix}`, isCodexManaged: true, isTemp: true }),
    );
    return path;
  },
  gitWorktreeRemove(_repoDir, path) {
    const index = previewWorktrees.findIndex((entry) => entry.path === path);
    if (index >= 0) previewWorktrees.splice(index, 1);
  },
  gitWorktreePrune() {
    for (let index = previewWorktrees.length - 1; index >= 0; index -= 1) {
      if (previewWorktrees[index].missingDir) previewWorktrees.splice(index, 1);
    }
  },
  gitWorktreeLock(_repoDir, path, reason) {
    const entry = previewWorktrees.find((candidate) => candidate.path === path);
    if (entry) {
      entry.locked = true;
      entry.lockReason = reason?.trim() || null;
    }
  },
  gitWorktreeUnlock(_repoDir, path) {
    const entry = previewWorktrees.find((candidate) => candidate.path === path);
    if (entry) {
      entry.locked = false;
      entry.lockReason = null;
    }
  },

  // --- CLI / desktop handoff and remote pairing ---
  handoffCommand: (threadId, cwd) =>
    `CODEX_HOME='${previewLaunchState.codexHome}' codex resume '${threadId}' --cd '${cwd}'`,
  handoffThreadLink(threadId, cwd, label) {
    const params = new URLSearchParams({ path: cwd, codexHome: previewLaunchState.codexHome });
    if (label) params.set("label", label);
    return `codex://threads/${threadId}?${params.toString()}`;
  },
  async copyText(text) {
    await navigator.clipboard?.writeText(text).catch(() => {});
  },
  remotePairingStart: () => ({
    qrSvg:
      '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="220" height="220"><rect width="10" height="10" fill="#fff"/><path d="M1 1h3v3H1zM6 1h3v3H6zM1 6h3v3H1zM6 6h1v1H6zM8 6h1v1H8zM6 8h1v1H6zM8 8h1v1H8z" fill="#000"/></svg>',
    pairingCode: "preview-pairing-code",
    manualPairingCode: "PREVIEW-1234",
    expiresAt: Date.now() / 1000 + 600,
  }),
  remotePairingStatus: () => ({ claimed: false }),

  // --- Pull-request review ---
  reviewProviderStatus: () => ({ ...previewProviderStatus }),
  reviewListPrs: () => structuredClone(previewPrs),
  reviewPrDetail: (repoDir, number) => deskPrDetail(repoDir, number),
  reviewCheckFresh: (_repoDir, _number, knownHead, knownUpdatedAt) => ({
    stale: false,
    remoteHead: knownHead,
    remoteUpdatedAt: knownUpdatedAt,
  }),
  reviewLocalDiff: () => structuredClone(previewPrDetail.files),
  reviewInbox: (repoDir) => deskInbox(repoDir),
  reviewMarkActivitySeen: (repoDir, numbers) => deskMarkActivitySeen(repoDir, numbers),
  reviewMutePr: (repoDir, number, headSha) => deskMutePr(repoDir, number, headSha),
  reviewStacks: (repoDir) => deskStacks(repoDir),
  reviewFetchPrs: (repoDir, numbers) => {
    const prs = deskInbox(repoDir).prs;
    return numbers.map((number) => ({ number, headSha: prs.find((pr) => pr.number === number)?.headSha ?? null }));
  },
  reviewDiff: () => ({ files: structuredClone(previewPrDetail.files), truncated: false }),
  reviewFile: () => ({ text: "export const VERSION = 1;\n", size: 26, truncated: false }),
  reviewChangesSince: (_repoDir, _number, _baseSha, headSha) => ({
    seenSha: null,
    headSha,
    fastForward: false,
    files: structuredClone(previewPrDetail.files),
    rangeDiff: null,
    truncated: false,
  }),
  reviewMarkSeen: () => {},
  reviewForgetPr: () => {},
  reviewEditComment: () => {},
  reviewDeleteComment: () => {},
  reviewMerge: () => {},
  reviewClose: () => {},
  reviewRetarget: () => {},
  reviewMarkReady: () => {},
  reviewCheckLogs: () => [],
  reviewRerunFailedChecks: () => {},
  reviewCreatePr: () => nextPreviewId(),
  ...repositoryCommands,
  reviewPendingReview: (repoDir, number) => deskPendingReview(repoDir, number),
  reviewSavePendingReview: (repoDir, number, review) => deskSavePendingReview(repoDir, number, review),
  reviewDiscardPendingReview: (repoDir, number) => deskDiscardPendingReview(repoDir, number),
  reviewSubmitPendingReview: (repoDir, number) => deskSubmitPendingReview(repoDir, number),
  reviewViewedFiles: (repoDir, number, files) => deskViewedFiles(repoDir, number, files),
  reviewSetViewed: (repoDir, number, files, viewed) => deskSetViewed(repoDir, number, files, viewed),
  reviewSaveDraft(provider, repo, prNumber, headSha, payload) {
    reviewDrafts.set(reviewDraftKey(provider, repo, prNumber), {
      headSha,
      payload,
      updatedAt: Math.floor(Date.now() / 1000),
    });
  },
  reviewLoadDraft: (provider, repo, prNumber) => reviewDrafts.get(reviewDraftKey(provider, repo, prNumber)) ?? null,
  reviewDeleteDraft(provider, repo, prNumber) {
    reviewDrafts.delete(reviewDraftKey(provider, repo, prNumber));
  },

  // --- Remote connections ---
  listConnections: () => previewConnections.map((connection) => ({ ...connection })),
  refreshConnections: () => previewConnections.map((connection) => ({ ...connection })),
  renameConnection(clientId, name) {
    const connection = previewConnections.find((entry) => entry.clientId === clientId);
    if (connection && name.trim()) connection.name = name.trim();
  },
  disconnectConnection: dropConnection,
  revokeConnection: dropConnection,

  // --- Quick chat ---
  getQuickShortcut: () => previewQuickShortcut.value,
  setQuickShortcut(accelerator) {
    previewQuickShortcut.value = accelerator.trim() || defaultQuickShortcut;
    return previewQuickShortcut.value;
  },

  // --- Integrations (MCP servers, skills, plugins) and memories ---
  listIntegrations: (cwds) => integrationView(cwds?.[0]),
  setIntegrationEnabled(kind, id, enabled, projectPath) {
    const key = `${kind}:${id}`;
    if (projectPath) {
      const overrides = projectOverrides.get(projectPath) ?? {};
      if (enabled == null) delete overrides[key];
      else overrides[key] = enabled;
      projectOverrides.set(projectPath, overrides);
    } else {
      const entry =
        kind === "mcp"
          ? previewIntegrations.mcpServers.find((row) => row.name === id)
          : previewIntegrations.plugins.find((row) => row.id === id);
      if (entry) entry.enabled = enabled ?? true;
    }
    return integrationView(projectPath);
  },
  listMcpServerStatus: () => previewMcpServerStatus(),
  mcpOauthLogin(name) {
    previewMcpOauthLogin(name);
  },
  listSkillsFor: () => ({ data: [{ skills: structuredClone(previewIntegrations.skills) }] }),
  setSkillEnabled(name, enabled, path) {
    const skill = previewIntegrations.skills.find((entry) => (path ? entry.path === path : entry.name === name));
    if (skill) skill.enabled = enabled;
  },
  readSkill: (path) => previewReadSkill(path),
  readFilePreview: (path) => previewFilePreview(path),
  createSkill: (name, description, body) => previewCreateSkill({ name, description, body }),
  deleteSkill: (path) => previewDeleteSkill(path),
  saveMcpServer(server) {
    previewSaveMcpServer({
      ...server,
      args: server.args ?? [],
      envKeys: server.envKeys ?? Object.keys(server.env ?? {}),
    });
    return structuredClone(previewIntegrations);
  },
  removeMcpServer(name) {
    previewIntegrations.mcpServers = previewIntegrations.mcpServers.filter((server) => server.name !== name);
    return structuredClone(previewIntegrations);
  },
  setMcpEnabled(name, enabled) {
    const server = previewIntegrations.mcpServers.find((entry) => entry.name === name);
    if (server) server.enabled = enabled;
    return structuredClone(previewIntegrations);
  },
  listMemories: () => previewListMemories(),
  readMemory: (_homeId, path) => previewReadMemory(path),
  writeMemory: (_homeId, path, contents) => previewWriteMemory(path, contents),
  deleteMemory: (_homeId, path) => previewDeleteMemory(path),
  setMemoryToggle: (homeId, key, value) => previewSetMemoryToggle(homeId, key, value),

  // --- History search and pagination ---
  searchThreads: (query, cursor, filter, generation) =>
    previewSearchThreads(query, cursor, filter ?? undefined, generation),
  listThreadsPage: (cursor, pageSize, archived, projectPath) =>
    previewThreadsPage(cursor, pageSize ?? 0, archived ?? false, projectPath),

  // --- Message log ---
  readWireLog: () => [...previewWireLog],
};

/** Run the fake for `name`, or resolve `undefined` when the preview has none. */
export async function runPreviewCommand(name: string, args: unknown[]): Promise<unknown> {
  const fake = previewCommands[name as keyof Commands] as ((...args: unknown[]) => unknown) | undefined;
  return fake?.(...args);
}

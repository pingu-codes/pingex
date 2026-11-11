import { SvelteMap } from "svelte/reactivity";
import { hostKey, hostLabel, sameHost } from "$lib/app/host";
import type { BootstrapData, HomeSnapshot, Host, ProfileBootstrap, UsageScope } from "$lib/bindings";
import { commandArguments, commands as nativeCommands } from "$lib/bindings";
import { isAnnounced } from "$lib/services/commandEffects";
import { emitCommandSettled } from "$lib/services/commandEvents";
import { isTauri } from "$lib/services/tauri";

const PREFIX = "pingex:";
type Identity = { homeKey: string; nativeId: string };
let profile: ProfileBootstrap | null = null;
let profileGeneration = 0;
const revision = new SvelteMap<string, number>();
let selected: () => { threadId?: string | null; projectPath?: string | null } = () => ({});
let nextRequest = -1;
const requests = new Map<number, { homeKey: string; nativeId: number }>();
const requestKeys = new Map<string, number>();

export function setHomeSelection(read: typeof selected): void {
  selected = read;
}
export function profileHomes(): HomeSnapshot[] {
  revision.get("profile");
  return profile?.homes ?? [];
}
export function belongsToProfile(key: string): boolean {
  return profile?.homes.some((entry) => entry.homeKey === key) ?? false;
}

export function scopedId(homeKey: string, nativeId: string): string {
  return PREFIX + JSON.stringify([homeKey, nativeId]);
}

export function identity(value: unknown): Identity | null {
  if (typeof value !== "string" || !value.startsWith(PREFIX)) return null;
  try {
    const parts: unknown = JSON.parse(value.slice(PREFIX.length));
    return Array.isArray(parts) && parts.length === 2 && parts.every((part) => typeof part === "string")
      ? { homeKey: parts[0], nativeId: parts[1] }
      : null;
  } catch {
    return null;
  }
}

/** Keep /mnt/c distinct from Windows C:, even though both access the same files. */
export function localProjectPath(host: Host, path: string): string {
  if (host.kind === "native" || !path.startsWith("/")) return path;
  return `\\\\wsl.localhost\\${host.distro}\\${path.replace(/^\/+/, "").replaceAll("/", "\\")}`;
}

function hostPath(host: Host, path: string): string {
  if (host.kind === "native") return path;
  for (const prefix of [`\\\\wsl.localhost\\${host.distro}\\`, `\\\\wsl$\\${host.distro}\\`]) {
    if (path.toLowerCase().startsWith(prefix.toLowerCase()))
      return `/${path.slice(prefix.length).replaceAll("\\", "/")}`;
  }
  return path;
}

function homeForPath(path: string, harness: string | null = "codex"): HomeSnapshot | undefined {
  const share = /^\\\\(?:wsl\.localhost|wsl\$)\\([^\\]+)(?:\\|$)/i.exec(path);
  const host = share ? `wsl:${share[1]}` : "native";
  const candidates = profileHomes().filter(
    (entry) =>
      hostKey(entry.home.host).toLowerCase() === host.toLowerCase() && entry.home.harness === (harness ?? "codex"),
  );
  return candidates.find((entry) => entry.home.isDefault) ?? candidates[0];
}

export function projectHostLabel(path: string): string {
  return hostLabel(homeForPath(path)?.home.host);
}

export function currentHomeKey(harness?: string): string {
  return selectedHome(harness)?.homeKey ?? "default";
}

export function threadHomeLabel(threadId: string): string {
  const entry = profileHomes().find((home) => home.homeKey === identity(threadId)?.homeKey);
  return entry ? `${entry.home.label} · ${hostLabel(entry.home.host) || "This computer"}` : "";
}

export function availableHomes(): HomeSnapshot[] {
  const current = selectedHome();
  return current ? profileHomes().filter((entry) => sameHost(entry.home.host, current.home.host)) : [];
}

export async function chooseDefaultHome(id: string): Promise<void> {
  await nativeCommands.setProfileDefaultHome(id);
  await loadProfile(false);
}

/** Repoint a Home. Reloading the Profile refreshes the list and the routing
 *  that `homeForPath` does, so both see the new directory at once. */
export async function updateHome(id: string, configDir: string, binary: string, label: string): Promise<void> {
  await nativeCommands.updateProfileHome(id, configDir, binary, label);
  await loadProfile(false);
}

export async function removeHome(id: string): Promise<void> {
  await nativeCommands.removeProfileHome(id);
  await loadProfile(false);
}

function selectedHome(harness?: string): HomeSnapshot | undefined {
  const current = selected();
  const thread = identity(current.threadId);
  if (thread) {
    const owner = profileHomes().find((entry) => entry.homeKey === thread.homeKey);
    if (owner && (!harness || owner.home.harness === harness)) return owner;
  }
  if (current.projectPath) return homeForPath(current.projectPath, harness);
  return profileHomes().find((entry) => entry.home.harness === (harness ?? "codex") && entry.home.isDefault);
}

function mapSummary<
  T extends {
    id: string;
    cwd?: string;
    parentThreadId?: string | null;
    projectId?: string | null;
    sectionId?: string | null;
  },
>(entry: HomeSnapshot, value: T): T {
  return {
    ...value,
    id: scopedId(entry.homeKey, value.id),
    ...(value.cwd === undefined ? {} : { cwd: localProjectPath(entry.home.host, value.cwd) }),
    ...(value.parentThreadId ? { parentThreadId: scopedId(entry.homeKey, value.parentThreadId) } : {}),
    ...(value.projectId ? { projectId: scopedId(entry.homeKey, value.projectId) } : {}),
    ...(value.sectionId ? { sectionId: scopedId(entry.homeKey, value.sectionId) } : {}),
  };
}

function mapBootstrap(entry: HomeSnapshot): BootstrapData {
  const id = (value: string) => (identity(value) ? value : scopedId(entry.homeKey, value));
  const path = (value: string) => localProjectPath(entry.home.host, value);
  const data = entry.data;
  const owned = new Set(
    [...data.projects.flatMap((project) => project.threads), ...data.subagents]
      .filter((thread) => (thread.harness ?? "codex") === entry.home.harness)
      .map((thread) => thread.id),
  );
  for (let changed = true; changed; ) {
    changed = false;
    for (const branch of data.threadBranches)
      if (owned.has(branch.parentThreadId) && !owned.has(branch.threadId)) {
        owned.add(branch.threadId);
        changed = true;
      }
  }
  return {
    ...data,
    projects: data.projects.map((project) => ({
      ...project,
      path: path(project.path),
      workspaceId: project.workspaceId ? id(project.workspaceId) : null,
      threads: project.threads
        .filter((thread) => (thread.harness ?? "codex") === entry.home.harness)
        .map((thread) => mapSummary(entry, thread)),
      sources: project.sources.map((source) => ({
        ...source,
        id: id(source.id),
        projectPath: path(source.projectPath),
      })),
      members: (project.members ?? []).map((member) => ({
        ...member,
        sourcePath: path(member.sourcePath),
        effectivePath: path(member.effectivePath),
      })),
    })),
    subagents: data.subagents
      .filter((thread) => (thread.harness ?? "codex") === entry.home.harness)
      .map((thread) => mapSummary(entry, thread)),
    sideQuestions: data.sideQuestions
      .filter((question) => owned.has(question.parentThreadId))
      .map((question) => ({
        ...question,
        parentThreadId: id(question.parentThreadId),
        sideThreadId: id(question.sideThreadId),
      })),
    threadBranches: data.threadBranches
      .filter((branch) => owned.has(branch.parentThreadId))
      .map((branch) => ({
        ...branch,
        threadId: id(branch.threadId),
        parentThreadId: id(branch.parentThreadId),
        groupTurnId: branch.groupTurnId,
      })),
    sections: data.sections.map((section) => ({ ...section, id: id(section.id) })),
    sidebarLayout: {
      folders: data.sidebarLayout.folders.map((folder) => ({
        ...folder,
        id: id(folder.id),
        parentId: folder.parentId ? id(folder.parentId) : null,
        scope: folder.scope ? path(folder.scope) : "",
      })),
      placements: data.sidebarLayout.placements.map((placement) => {
        const isPath = /^[\\/]|^[a-z]:[\\/]/i.test(placement.itemKey);
        return {
          ...placement,
          scope: placement.scope ? path(placement.scope) : "",
          parentId: placement.parentId ? id(placement.parentId) : null,
          itemKey: isPath ? path(placement.itemKey) : id(placement.itemKey),
        };
      }),
    },
  };
}

function merged(): BootstrapData {
  const entries = profileHomes();
  if (!entries.length) throw new Error("Profile contains no Homes");
  const mapped = entries.map(mapBootstrap);
  const first = mapped[0];
  const projects = new Map<string, BootstrapData["projects"][number]>();
  for (const data of mapped)
    for (const project of data.projects) {
      const existing = projects.get(project.path);
      if (!existing) projects.set(project.path, project);
      else
        existing.threads.push(
          ...project.threads.filter((thread) => !existing.threads.some((candidate) => candidate.id === thread.id)),
        );
    }
  return {
    ...first,
    projects: [...projects.values()],
    subagents: mapped.flatMap((data) => data.subagents),
    sideQuestions: mapped.flatMap((data) => data.sideQuestions),
    threadBranches: mapped.flatMap((data) => data.threadBranches),
    sections: mapped.flatMap((data) => data.sections),
    sidebarLayout: first.sidebarLayout,
  };
}

export function acceptProfile(value: ProfileBootstrap): BootstrapData {
  profile = value;
  revision.set("profile", (revision.get("profile") ?? 0) + 1);
  return merged();
}

export async function loadProfile(refresh: boolean): Promise<BootstrapData> {
  const generation = profileGeneration;
  const result = await nativeCommands.bootstrapProfile(refresh);
  if (generation !== profileGeneration) throw new Error("Profile changed while loading");
  return acceptProfile(result);
}

export async function addProfileProject(path: string, host: Host | null): Promise<BootstrapData> {
  return acceptProfile(await nativeCommands.addProfileProject(path, host));
}

const idArguments = new Set([
  "threadId",
  "parentThreadId",
  "sideThreadId",
  "threadIds",
  "workspaceId",
  "sectionId",
  "parentId",
  "id",
  "runId",
  "hide",
  "collapseFolders",
]);
const pathArguments = new Set([
  "cwd",
  "cwds",
  "sourcePath",
  "path",
  "projectPath",
  "dir",
  "repoDir",
  "root",
  "project",
  "worktreePath",
  "targetDir",
  "scope",
  "collapseProjects",
]);
const objectArguments = new Set(["input", "item", "siblings"]);
const projectMutations = new Set([
  "renameProject",
  "setProjectPinned",
  "setProjectArchived",
  "setProjectExpanded",
  "removeProject",
  "saveProjectInstructions",
]);

function argumentIds(value: unknown): Identity[] {
  if (Array.isArray(value)) return value.flatMap(argumentIds);
  const found = identity(value);
  return found ? [found] : [];
}

function unwrap(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(unwrap);
  return identity(value)?.nativeId ?? value;
}

function mapStructured(value: unknown, host: Host): unknown {
  if (Array.isArray(value)) return value.map((item) => mapStructured(item, host));
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.entries(value).map(([key, item]) => {
      if (idArguments.has(key)) return [key, unwrap(item)];
      if (pathArguments.has(key) || key === "sourcePath")
        return [key, typeof item === "string" ? hostPath(host, item) : item];
      if (key === "members") return [key, mapStructured(item, host)];
      return [key, item];
    }),
  );
}

/** A command result is JSON of a shape keyed by the command's name. */
type JsonRecord = Record<string, unknown>;
type Summary = Parameters<typeof mapSummary>[1];
type CommandName = keyof typeof nativeCommands;
const asRecord = (value: unknown): JsonRecord | null =>
  value !== null && typeof value === "object" && !Array.isArray(value) ? (value as JsonRecord) : null;

type ResultMapper = (entry: HomeSnapshot, result: unknown) => unknown;
/** Map each element of an array result; anything else passes through. */
const each =
  <T>(map: (entry: HomeSnapshot, item: T) => unknown): ResultMapper =>
  (entry, result) =>
    Array.isArray(result) ? result.map((item: T) => map(entry, item)) : result;
const summaryResult: ResultMapper = (entry, result) =>
  asRecord(result)?.id ? mapSummary(entry, result as Summary) : result;
const pageResult: ResultMapper = (entry, result) => {
  const record = asRecord(result);
  return Array.isArray(record?.items)
    ? { ...record, items: (record.items as Summary[]).map((item) => mapSummary(entry, item)) }
    : result;
};
const scopedIdResult: ResultMapper = (entry, result) => {
  const record = asRecord(result);
  return typeof record?.id === "string" ? { ...record, id: scopedId(entry.homeKey, record.id) } : result;
};
const repoResult =
  (withParent: boolean): ResultMapper =>
  (entry, result) => {
    const record = asRecord(result);
    if (!record) return result;
    const path = (value: unknown) =>
      typeof value === "string" && value ? localProjectPath(entry.home.host, value) : null;
    return {
      ...record,
      ...(withParent ? { parentPath: path(record.parentPath) } : {}),
      dir: typeof record.dir === "string" ? localProjectPath(entry.home.host, record.dir) : record.dir,
      root: path(record.root),
      commonDir: path(record.commonDir),
    };
  };

/** How a command's result names Home-local things, rewritten into the
 *  Profile's scoped ids and local paths. Bootstrap snapshots are recognised
 *  by shape, whatever the command. */
const resultMappers: Partial<Record<CommandName, ResultMapper>> = {
  startThread: summaryResult,
  readThread: summaryResult,
  forkThread: summaryResult,
  revertThread: summaryResult,
  listSubagents: each(mapSummary<Summary>),
  listProjectSources: each((entry, source: { id: string }) => ({ ...source, id: scopedId(entry.homeKey, source.id) })),
  stageAttachment: scopedIdResult,
  stageClipboardImage: scopedIdResult,
  gitWorktrees: each((entry, tree: { path: string }) => ({
    ...tree,
    path: localProjectPath(entry.home.host, tree.path),
  })),
  gitRepoInfo: repoResult(false),
  gitContext: repoResult(true),
  listThreadsPage: pageResult,
};

function normalizeResult(name: string, entry: HomeSnapshot, result: unknown): unknown {
  const record = asRecord(result);
  if (record && Array.isArray(record.projects) && record.sidebarLayout) {
    entry.data = record as unknown as BootstrapData;
    return merged();
  }
  const map = resultMappers[name as CommandName];
  return map ? map(entry, result) : result;
}

/** One intercepted command call: its name, the generated function, the
 *  caller's arguments and the generated argument names. */
type Call = { name: string; method: (...args: unknown[]) => unknown; args: unknown[]; names: readonly string[] };

const invoke = (call: Call, args = call.args) => Reflect.apply(call.method, nativeCommands, args);

/** `args` with the trailing `window` argument pointed at one Home. */
function onHome(call: Call, args: unknown[], homeKey: string): unknown[] {
  const routed = [...args];
  routed[call.names.indexOf("window")] = { homeKey };
  return routed;
}

const profileRoot = () => profileHomes().find((home) => home.homeKey === profile!.profileKey)!;
const ownerOf = (id: string | undefined) => profileHomes().find((home) => home.homeKey === identity(id)?.homeKey);

/** Launch and runtime settings belong to the window, not a Home. Switching
 *  the Codex home drops the Profile so the next bootstrap reloads it. */
async function routeWindowCommand(call: Call): Promise<unknown> {
  const result = await invoke(call);
  if (call.name === "selectCodexHome") {
    profile = null;
    profileGeneration++;
    requests.clear();
    requestKeys.clear();
    revision.set("profile", (revision.get("profile") ?? 0) + 1);
  }
  return result;
}

/** The scope is an object, not a bare id or path: route by what it names. */
async function routeUsageBreakdown(call: Call): Promise<unknown> {
  const [scope, since] = call.args as [
    { kind: "thread"; threadId: string } | { kind: "project"; path: string } | { kind: "global" },
    number | null,
  ];
  const owner =
    scope.kind === "thread"
      ? ownerOf(scope.threadId)
      : scope.kind === "project"
        ? homeForPath(scope.path)
        : selectedHome();
  if (!owner) throw new Error("Choose a Home for this project");
  const nativeScope: UsageScope =
    scope.kind === "thread"
      ? { kind: "thread", threadId: unwrap(scope.threadId) as string }
      : scope.kind === "project"
        ? { kind: "project", path: hostPath(owner.home.host, scope.path) }
        : scope;
  const breakdown = await nativeCommands.readUsageBreakdown(nativeScope, since, { homeKey: owner.homeKey });
  return {
    ...breakdown,
    byThread: breakdown.byThread.map((row) => ({ ...row, threadId: scopedId(owner.homeKey, row.threadId) })),
  };
}

/** Search every Home on the project's Host (or every Home) and merge the
 *  pages; the cursor carries each Home's own cursor. */
async function routeSearchThreads(call: Call): Promise<unknown> {
  const [query, cursor, filter, generation] = call.args as [
    string,
    string | null,
    { archived?: boolean; projectPath?: string | null } | null,
    number,
  ];
  const continuation: { cursors: Record<string, string | null>; total: number } | null = cursor
    ? JSON.parse(cursor)
    : null;
  const cursors = continuation?.cursors;
  const project = filter?.projectPath;
  const owner = project ? homeForPath(project) : null;
  const entries = profileHomes().filter(
    (home) => (!owner || sameHost(home.home.host, owner.home.host)) && (!cursors || home.homeKey in cursors),
  );
  const pages = await Promise.all(
    entries.map(async (home) => ({
      home,
      page: await nativeCommands.searchThreads(
        query,
        cursors?.[home.homeKey] ?? null,
        filter ? { ...filter, projectPath: project ? hostPath(home.home.host, project) : null } : null,
        generation,
        { homeKey: home.homeKey },
      ),
    })),
  );
  const next = Object.fromEntries(
    pages.filter(({ page }) => page.nextCursor !== null).map(({ home, page }) => [home.homeKey, page.nextCursor]),
  );
  const total = continuation?.total ?? pages.reduce((sum, { page }) => sum + page.total, 0);
  return {
    items: pages
      .flatMap(({ home, page }) => page.items.map((item) => mapSummary(home, item)))
      .sort((a, b) => b.updatedAt - a.updatedAt),
    nextCursor: Object.keys(next).length ? JSON.stringify({ cursors: next, total }) : null,
    total,
    generation,
  };
}

/** Hide/collapse across the whole Profile: each Home gets the ids and paths
 *  it owns; folders live on the Profile's root Home. */
async function routeToEveryHome(call: Call): Promise<unknown> {
  for (const home of profileHomes()) {
    const perHome = call.args.map((value, index) => {
      if (!Array.isArray(value)) return value;
      const name = call.names[index];
      if (name === "collapseProjects")
        return value
          .filter((path) => {
            const owner = homeForPath(path);
            return owner && sameHost(owner.home.host, home.home.host);
          })
          .map((path) => hostPath(home.home.host, path));
      if (name === "collapseFolders") return home.homeKey === profile!.profileKey ? value.map(unwrap) : [];
      return value.filter((id) => identity(id)?.homeKey === home.homeKey).map(unwrap);
    });
    normalizeResult(call.name, home, await invoke(call, onHome(call, perHome, home.homeKey)));
  }
  return merged();
}

/** The sidebar layout is the Profile's, stored on its root Home. */
async function routeToProfileRoot(call: Call): Promise<unknown> {
  const root = profileRoot();
  const routed = call.args.map((value, index) => {
    const name = call.names[index];
    if (["id", "parentId"].includes(name)) return unwrap(value);
    if (name === "item" || name === "siblings") {
      const reference = (item: unknown) => {
        const record = asRecord(item);
        return record?.kind === "folder" ? { ...record, id: unwrap(record.id) } : item;
      };
      return Array.isArray(value) ? value.map(reference) : reference(value);
    }
    return value;
  });
  return normalizeResult(call.name, root, await invoke(call, onHome(call, routed, root.homeKey)));
}

/** Thread-id lists gathered from every Home. */
async function collectFromEveryHome(call: Call): Promise<unknown> {
  const results = await Promise.all(
    profileHomes().map(async (entry) => {
      const values = await invoke(call, [{ homeKey: entry.homeKey }]);
      return (values as string[]).map((value) => scopedId(entry.homeKey, value));
    }),
  );
  return results.flat();
}

/** The Home a command runs on when it names no thread or path. */
const defaultHarness: Partial<Record<CommandName, string>> = {
  listModels: "codex",
  readAccountRateLimits: "codex",
  readCodexServerInfo: "codex",
};

/** Everything else runs on one Home: the owner of the ids it names, else
 *  of the paths, else the selection's. */
async function routeToOwner(call: Call): Promise<unknown> {
  const { name: property, args, names } = call;
  const choice = args[names.indexOf("harness")];
  const harness = typeof choice === "string" ? choice : defaultHarness[property as CommandName];
  const identities = names.flatMap((name, index) => (idArguments.has(name) ? argumentIds(args[index]) : []));
  const keys = new Set(identities.map((item) => item.homeKey));
  if (identities.some((item) => !belongsToProfile(item.homeKey)))
    throw new Error("Conversation does not belong to this Profile");
  if (keys.size > 1 && property !== "moveThreadToWorkspace") throw new Error("This operation spans multiple Homes");
  const request = requests.get(args[names.indexOf("requestId")] as number);
  let entry = profileHomes().find((home) => home.homeKey === (request?.homeKey ?? identities[0]?.homeKey));
  const workspace = ["createWorkspace", "updateWorkspace"].includes(property)
    ? (args[names.indexOf("input")] as { members?: { sourcePath: string }[]; workspaceId?: string })
    : undefined;
  if (workspace?.workspaceId) entry = ownerOf(workspace.workspaceId);
  if (!entry && workspace?.members?.[0]) entry = homeForPath(workspace.members[0].sourcePath);
  if (!entry) {
    const pathIndex = names.findIndex(
      (name, index) => pathArguments.has(name) && typeof args[index] === "string" && args[index] !== "",
    );
    if (pathIndex >= 0) entry = homeForPath(args[pathIndex] as string, harness);
  }
  entry ??= selectedHome(harness);
  if (!entry) throw new Error("Choose a Home for this project");
  if (property === "startThread") {
    const cwd = args[names.indexOf("cwd")];
    if (typeof cwd === "string") entry = homeForPath(cwd, harness) ?? entry;
  }
  const host = entry.home.host;
  if (
    workspace?.members?.some((member) => {
      const owner = homeForPath(member.sourcePath);
      return !owner || !sameHost(owner.home.host, host);
    })
  )
    throw new Error("A workspace can only contain projects on the same Host");
  const workspaceRef = identity(args[names.indexOf("workspaceId")]);
  if (workspaceRef && ["startThread", "moveThreadToWorkspace"].includes(property)) {
    await nativeCommands.prepareProfileWorkspace(workspaceRef.homeKey, workspaceRef.nativeId, entry.homeKey);
  }
  if (property === "startTurn") {
    const nativeId = identities[0]?.nativeId;
    const project = entry.data.projects.find(
      (project) => project.workspaceId && project.threads.some((thread) => thread.id === nativeId),
    );
    if (project?.workspaceId) {
      const owner = profileHomes().find(
        (home) =>
          sameHost(home.home.host, host) &&
          home.data.projects.some((candidate) => candidate.workspaceId === project.workspaceId),
      );
      if (owner && owner.homeKey !== entry.homeKey)
        await nativeCommands.prepareProfileWorkspace(owner.homeKey, project.workspaceId, entry.homeKey);
    }
  }
  const routed = args.map((value, index) => {
    const name = names[index];
    if (name === "requestId" && request) return request.nativeId;
    if (idArguments.has(name)) return unwrap(value);
    if (pathArguments.has(name))
      return Array.isArray(value)
        ? value.map((path) => hostPath(host, path))
        : typeof value === "string"
          ? hostPath(host, value)
          : value;
    if (property === "gitWorktreeAdd" && name === "request") return mapStructured(value, host);
    // Only workspace/sidebar inputs have paths and entity references.
    if (objectArguments.has(name) && ["createWorkspace", "updateWorkspace", "placeSidebarItem"].includes(property))
      return mapStructured(value, host);
    return value;
  });
  if (projectMutations.has(property)) {
    // Project settings are stored per Home: apply them on every Home of the Host.
    let last: unknown;
    for (const home of profileHomes().filter((home) => sameHost(home.home.host, host)))
      last = normalizeResult(property, home, await invoke(call, onHome(call, routed, home.homeKey)));
    return last;
  }
  const result = await invoke(call, onHome(call, routed, entry.homeKey));
  if (request && ["respondApproval", "respondUserInput", "respondServerRequest"].includes(property)) {
    requests.delete(args[names.indexOf("requestId")] as number);
    requestKeys.delete(JSON.stringify([request.homeKey, request.nativeId]));
  }
  return normalizeResult(property, entry, result);
}

/** Commands that do not take the one-owner route. */
const routers: Partial<Record<CommandName, (call: Call) => Promise<unknown>>> = {
  readUsageBreakdown: routeUsageBreakdown,
  searchThreads: routeSearchThreads,
  setThreadsHidden: routeToEveryHome,
  applySessionFocus: routeToEveryHome,
  createSidebarFolder: routeToProfileRoot,
  renameSidebarFolder: routeToProfileRoot,
  deleteSidebarFolder: routeToProfileRoot,
  setSidebarFolderExpanded: routeToProfileRoot,
  placeSidebarItem: routeToProfileRoot,
  resetSidebarOrder: routeToProfileRoot,
  threadsWithActiveTurns: collectFromEveryHome,
  threadsWithUnansweredQuestions: collectFromEveryHome,
};

/** Commands bound to the window rather than a Home; they run before a Profile exists. */
const windowCommands = new Set<string>([
  "readLaunchState",
  "selectCodexHome",
  "removeRecentHome",
  "updateRuntimeSettings",
  "setCodexBinary",
  "readRuntimeSettings",
]);

/** The browser preview (the dev server outside Tauri, which Playwright drives)
 *  answers commands from in-memory fixtures. Production builds compile the
 *  import away, so the app bundle never carries them. */
async function previewCommand(name: string, args: unknown[]): Promise<unknown> {
  return import.meta.env.DEV
    ? (await import("$lib/services/preview/commands")).runPreviewCommand(name, args)
    : undefined;
}

/** Generated commands remain the only IPC entry point. Argument metadata
 * identifies routing fields, so user text and tool payloads are never rewritten.
 * Outside Tauri every command goes to the browser preview's fakes instead. */
export const commands: typeof nativeCommands = new Proxy(nativeCommands, {
  get(target, property: string) {
    const method = target[property as keyof typeof target];
    if (typeof method !== "function") return method;
    const names: readonly string[] = commandArguments[property as keyof typeof commandArguments] ?? [];
    // Settled git and review commands are announced so what they changed
    // catches up (`commandEffects.ts`).
    const announced = isAnnounced(property);
    const announce = <T>(args: unknown[], result: T | Promise<T>): Promise<T> => {
      const settled = Promise.resolve(result);
      return announced ? settled.finally(() => emitCommandSettled({ name: property, args, names })) : settled;
    };
    if (!isTauri()) return (...args: unknown[]) => announce(args, previewCommand(property, args));
    if (!names.includes("window")) return method;
    return async (...args: unknown[]) => {
      const call: Call = { name: property, method: method as Call["method"], args, names };
      if (property === "bootstrap") return loadProfile(true);
      if (windowCommands.has(property)) return routeWindowCommand(call);
      if (!profile) return announce(args, invoke(call));
      return announce(args, (routers[property as CommandName] ?? routeToOwner)(call));
    };
  },
});

function scopedRequest(homeKey: string, nativeId: number): number {
  const key = JSON.stringify([homeKey, nativeId]);
  const existing = requestKeys.get(key);
  if (existing !== undefined) return existing;
  const id = nextRequest--;
  requestKeys.set(key, id);
  requests.set(id, { homeKey, nativeId });
  return id;
}

export function scopeEvent<T>(homeKey: string, original: T): T {
  if (!belongsToProfile(homeKey)) return original;
  const payload = asRecord(structuredClone(original));
  if (!payload) return original;
  const entry = profileHomes().find((home) => home.homeKey === homeKey)!;
  const fields = (value: unknown) => {
    const object = asRecord(value);
    if (!object) return;
    for (const key of [
      "threadId",
      "thread_id",
      "parentThreadId",
      "parent_thread_id",
      "agentThreadId",
      "childThreadId",
      "child_thread_id",
      "runId",
      "senderThreadId",
    ]) {
      const id = object[key];
      if (typeof id === "string") object[key] = scopedId(homeKey, id);
    }
    if (typeof object.requestId === "number") object.requestId = scopedRequest(homeKey, object.requestId);
    const thread = asRecord(object.thread);
    if (thread?.id) object.thread = mapSummary(entry, thread as Summary);
    if (Array.isArray(object.receiverThreadIds))
      object.receiverThreadIds = object.receiverThreadIds.map((id: string) => scopedId(homeKey, id));
  };
  const params = asRecord(payload.params);
  fields(payload);
  fields(params);
  fields(asRecord(payload.event)?.params);
  fields(params?.item);
  const run = asRecord(payload.run);
  if (run) {
    fields(run);
    if (typeof run.id === "string") run.id = scopedId(homeKey, run.id);
  }
  return payload as T;
}

export function threadBelongsToHome(threadId: string, homeKey: string): boolean {
  return identity(threadId)?.homeKey === homeKey;
}

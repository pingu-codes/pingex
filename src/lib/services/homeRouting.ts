import { SvelteMap } from "svelte/reactivity";
import type { BootstrapData, HomeSnapshot, Host, ProfileBootstrap, UsageScopeArg } from "$lib/bindings";
import { commandArguments, commands as nativeCommands } from "$lib/bindings";

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

function hostKey(host: Host): string {
  return host.kind === "native" ? "native" : `wsl:${host.distro}`;
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
  const host = homeForPath(path)?.home.host;
  return host?.kind === "wsl" ? `WSL · ${host.distro}` : "";
}

export function currentHomeKey(harness?: string): string {
  return selectedHome(harness)?.homeKey ?? "default";
}

export function threadHomeLabel(threadId: string): string {
  const entry = profileHomes().find((home) => home.homeKey === identity(threadId)?.homeKey);
  return entry
    ? `${entry.home.label} · ${entry.home.host.kind === "wsl" ? `WSL · ${entry.home.host.distro}` : "This computer"}`
    : "";
}

export function availableHomes(): HomeSnapshot[] {
  const current = selectedHome();
  return current ? profileHomes().filter((entry) => hostKey(entry.home.host) === hostKey(current.home.host)) : [];
}

export async function chooseDefaultHome(id: string): Promise<void> {
  await nativeCommands.setProfileDefaultHome(id);
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
const profileCommands = new Set([
  "readLaunchState",
  "selectCodexHome",
  "removeRecentHome",
  "updateRuntimeSettings",
  "setCodexBinary",
  "readRuntimeSettings",
]);
const projectMutations = new Set([
  "renameProject",
  "setProjectPinned",
  "setProjectArchived",
  "setProjectExpanded",
  "removeProject",
  "saveProjectInstructions",
]);
const sidebarCommands = new Set([
  "createSidebarFolder",
  "renameSidebarFolder",
  "deleteSidebarFolder",
  "setSidebarFolderExpanded",
  "placeSidebarItem",
  "resetSidebarOrder",
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

function normalizeResult(name: string, entry: HomeSnapshot, result: any): any {
  if (result && Array.isArray(result.projects) && result.sidebarLayout) {
    entry.data = result;
    return merged();
  }
  if (["startThread", "readThread", "forkThread", "rollbackThread", "revertThread"].includes(name) && result?.id)
    return mapSummary(entry, result);
  if (name === "listSubagents" && Array.isArray(result)) return result.map((item) => mapSummary(entry, item));
  if (["threadsWithActiveTurns", "threadsWithUnansweredQuestions"].includes(name))
    return result.map((id: string) => scopedId(entry.homeKey, id));
  if (name === "listProjectSources" && Array.isArray(result))
    return result.map((source) => ({ ...source, id: scopedId(entry.homeKey, source.id) }));
  if (["stageAttachment", "stageClipboardImage"].includes(name) && result?.id)
    return { ...result, id: scopedId(entry.homeKey, result.id) };
  if (name === "gitWorktrees" && Array.isArray(result))
    return result.map((tree) => ({ ...tree, path: localProjectPath(entry.home.host, tree.path) }));
  if (["gitRepoInfo", "gitContext"].includes(name) && result)
    return {
      ...result,
      ...(name === "gitContext"
        ? { parentPath: result.parentPath ? localProjectPath(entry.home.host, result.parentPath) : null }
        : {}),
      dir: localProjectPath(entry.home.host, result.dir),
      root: result.root ? localProjectPath(entry.home.host, result.root) : null,
      commonDir: result.commonDir ? localProjectPath(entry.home.host, result.commonDir) : null,
    };
  if (name === "listArchivedThreads" && Array.isArray(result?.data))
    return { ...result, data: result.data.map((item: any) => mapSummary(entry, item)) };
  if (["listThreadsPage", "searchThreads"].includes(name) && Array.isArray(result?.items))
    return { ...result, items: result.items.map((item: any) => mapSummary(entry, item)) };
  if (name === "listAgentRuns" && Array.isArray(result))
    return result.map((run) => scopeEvent(entry.homeKey, { run }).run);
  if (name === "openAgentThread" && result?.id) return mapSummary(entry, result);
  return result;
}

/** Generated commands remain the only IPC entry point. Argument metadata
 * identifies routing fields, so user text and tool payloads are never rewritten. */
export const commands: typeof nativeCommands = new Proxy(nativeCommands, {
  get(target, property: string) {
    const method = target[property as keyof typeof target];
    if (typeof method !== "function") return method;
    const names: readonly string[] = commandArguments[property as keyof typeof commandArguments] ?? [];
    if (!names.includes("window")) return method;
    return async (...args: unknown[]) => {
      if (property === "bootstrap") return loadProfile(true);
      if (profileCommands.has(property)) {
        const result = await Reflect.apply(method, target, args);
        if (property === "selectCodexHome") {
          profile = null;
          profileGeneration++;
          requests.clear();
          requestKeys.clear();
          revision.set("profile", (revision.get("profile") ?? 0) + 1);
        }
        return result;
      }
      if (!profile) return Reflect.apply(method, target, args);
      if (property === "readUsageBreakdown") {
        // The scope is an object, not a bare id or path: route by what it names.
        const [scope, since] = args as [
          { kind: "thread"; threadId: string } | { kind: "project"; path: string } | { kind: "global" },
          number | null,
        ];
        const owner =
          scope.kind === "thread"
            ? profileHomes().find((home) => home.homeKey === identity(scope.threadId)?.homeKey)
            : scope.kind === "project"
              ? homeForPath(scope.path)
              : selectedHome();
        if (!owner) throw new Error("Choose a Home for this project");
        const nativeScope: UsageScopeArg =
          scope.kind === "thread"
            ? { kind: "thread", threadId: unwrap(scope.threadId) as string }
            : scope.kind === "project"
              ? { kind: "project", path: hostPath(owner.home.host, scope.path) }
              : scope;
        const breakdown = await nativeCommands.readUsageBreakdown(nativeScope, since, {
          homeKey: owner.homeKey,
        });
        return {
          ...breakdown,
          byThread: breakdown.byThread.map((row) => ({ ...row, threadId: scopedId(owner.homeKey, row.threadId) })),
        };
      }
      if (property === "searchThreads") {
        const [query, cursor, filter, generation] = args as [
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
          (home) =>
            (!owner || hostKey(home.home.host) === hostKey(owner.home.host)) && (!cursors || home.homeKey in cursors),
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
      if (["setThreadsHidden", "applySessionFocus"].includes(property)) {
        for (const home of profileHomes()) {
          const perHome = args.map((value, index) => {
            if (!Array.isArray(value)) return value;
            const name = names[index];
            if (name === "collapseProjects")
              return value
                .filter(
                  (path) =>
                    homeForPath(path)?.home.host && hostKey(homeForPath(path)!.home.host) === hostKey(home.home.host),
                )
                .map((path) => hostPath(home.home.host, path));
            if (name === "collapseFolders") return home.homeKey === profile!.profileKey ? value.map(unwrap) : [];
            return value.filter((id) => identity(id)?.homeKey === home.homeKey).map(unwrap);
          });
          perHome[names.indexOf("window")] = { homeKey: home.homeKey };
          normalizeResult(property, home, await Reflect.apply(method, target, perHome));
        }
        return merged();
      }
      if (sidebarCommands.has(property)) {
        const root = profileHomes().find((home) => home.homeKey === profile!.profileKey)!;
        const routed = args.map((value, index) => {
          const name = names[index];
          if (["id", "parentId"].includes(name)) return unwrap(value);
          if (name === "item" || name === "siblings") {
            const reference = (item: any) => (item.kind === "folder" ? { ...item, id: unwrap(item.id) } : item);
            return Array.isArray(value) ? value.map(reference) : reference(value);
          }
          return value;
        });
        routed[names.indexOf("window")] = { homeKey: root.homeKey };
        return normalizeResult(property, root, await Reflect.apply(method, target, routed));
      }
      if (["threadsWithActiveTurns", "threadsWithUnansweredQuestions"].includes(property)) {
        const results = await Promise.all(
          profileHomes().map(async (entry) => {
            const values = await Reflect.apply(method, target, [{ homeKey: entry.homeKey }]);
            return (values as string[]).map((value) => scopedId(entry.homeKey, value));
          }),
        );
        return results.flat();
      }
      const choice = args[names.indexOf("harness")];
      const harness =
        typeof choice === "string"
          ? choice
          : property === "readClaudeStatus"
            ? "claude"
            : ["listModels", "readAccountRateLimits", "readCodexServerInfo"].includes(property)
              ? "codex"
              : undefined;
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
      if (workspace?.workspaceId)
        entry = profileHomes().find((home) => home.homeKey === identity(workspace.workspaceId)?.homeKey);
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
      if (
        workspace?.members?.some((member) => {
          const owner = homeForPath(member.sourcePath);
          return !owner || hostKey(owner.home.host) !== hostKey(entry!.home.host);
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
              hostKey(home.home.host) === hostKey(entry!.home.host) &&
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
            ? value.map((path) => hostPath(entry!.home.host, path))
            : typeof value === "string"
              ? hostPath(entry!.home.host, value)
              : value;
        if (property === "gitWorktreeAdd" && name === "request") return mapStructured(value, entry!.home.host);
        // Only workspace/sidebar inputs have paths and entity references.
        if (objectArguments.has(name) && ["createWorkspace", "updateWorkspace", "placeSidebarItem"].includes(property))
          return mapStructured(value, entry!.home.host);
        return value;
      });
      routed[names.indexOf("window")] = { homeKey: entry.homeKey };
      if (projectMutations.has(property)) {
        let last: unknown;
        for (const home of profileHomes().filter((home) => hostKey(home.home.host) === hostKey(entry.home.host))) {
          const perHome = [...routed];
          perHome[names.indexOf("window")] = { homeKey: home.homeKey };
          last = normalizeResult(property, home, await Reflect.apply(method, target, perHome));
        }
        return last;
      }
      const result = await Reflect.apply(method, target, routed);
      if (request && ["respondApproval", "respondUserInput", "respondServerRequest"].includes(property)) {
        requests.delete(args[names.indexOf("requestId")] as number);
        requestKeys.delete(JSON.stringify([request.homeKey, request.nativeId]));
      }
      return normalizeResult(property, entry, result);
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
  const payload: any = structuredClone(original);
  const entry = profileHomes().find((home) => home.homeKey === homeKey)!;
  const fields = (object: any) => {
    if (!object || typeof object !== "object") return;
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
      if (typeof object[key] === "string") object[key] = scopedId(homeKey, object[key]);
    }
    if (typeof object.requestId === "number") object.requestId = scopedRequest(homeKey, object.requestId);
    if (object.thread?.id) object.thread = mapSummary(entry, object.thread);
    if (Array.isArray(object.receiverThreadIds))
      object.receiverThreadIds = object.receiverThreadIds.map((id: string) => scopedId(homeKey, id));
  };
  fields(payload);
  fields(payload.params);
  fields(payload.event?.params);
  fields(payload.params?.item);
  if (payload.run) {
    fields(payload.run);
    if (typeof payload.run.id === "string") payload.run.id = scopedId(homeKey, payload.run.id);
  }
  return payload as T;
}

export function threadBelongsToHome(threadId: string, homeKey: string): boolean {
  return identity(threadId)?.homeKey === homeKey;
}

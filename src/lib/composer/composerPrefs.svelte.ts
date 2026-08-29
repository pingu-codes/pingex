import type { SubagentPolicy, TurnOptions } from "$lib/types";

/**
 * The Subagents popover's choices. Held apart from the rest of the prefs
 * because they carry globally: the last-used set becomes the default for a
 * project nobody has configured, where model/effort/permission stay per-project.
 */
export interface SubagentPrefs {
  /**
   * Whether new threads get the app's own agent tools. `null` follows the
   * global setting. Only consulted when a thread is created: `dynamicTools` is
   * accepted on `thread/start` and nowhere else, so an existing thread keeps
   * whatever it was started with.
   */
  appSubagents: boolean | null;
  /** Models subagents may run on; `null` allows every model. */
  subagentModelPolicy: SubagentPolicy | null;
  /** Reasoning efforts subagents may run at; `null` allows every effort. */
  subagentReasoningEffortPolicy: SubagentPolicy | null;
}

export type HarnessChoice = "codex" | "claude";

/** Composer model/effort/permission choices, persisted in localStorage. */
export interface ComposerPrefs extends SubagentPrefs {
  /** Which harness a new thread starts on; `null` means Codex. Only read
   *  when a draft is sent: an existing thread never switches. */
  harness: HarnessChoice | null;
  model: string | null;
  effort: string | null;
  permissionPreset: string | null;
  planMode: boolean;
}

export interface PermissionPreset {
  id: string;
  label: string;
  description: string;
  approvalPolicy: string;
  sandboxMode: string;
}

export const PERMISSION_PRESETS: PermissionPreset[] = [
  {
    id: "read-only",
    label: "Read Only",
    description: "Codex can read files; edits and commands need approval",
    approvalPolicy: "on-request",
    sandboxMode: "read-only",
  },
  {
    id: "auto",
    label: "Auto",
    description: "Codex can edit files in the workspace and asks when unsure",
    approvalPolicy: "on-request",
    sandboxMode: "workspace-write",
  },
  {
    id: "full-access",
    label: "Full Access",
    description: "No sandbox and no approval prompts — use with care",
    approvalPolicy: "never",
    sandboxMode: "danger-full-access",
  },
];

/** The same three levels on Claude, in its own words. The ids match Codex's
 *  so a preset choice carries across harnesses; the driver maps each onto a
 *  Claude permission mode. */
export const CLAUDE_PERMISSION_PRESETS: PermissionPreset[] = [
  {
    id: "read-only",
    label: "Ask",
    description: "Claude asks before every edit and command",
    approvalPolicy: "on-request",
    sandboxMode: "read-only",
  },
  {
    id: "auto",
    label: "Accept edits",
    description: "Claude edits files without asking; commands still need approval",
    approvalPolicy: "on-request",
    sandboxMode: "workspace-write",
  },
  {
    id: "full-access",
    label: "Bypass permissions",
    description: "No prompts at all. Use with care",
    approvalPolicy: "never",
    sandboxMode: "danger-full-access",
  },
];

export function permissionPresetsFor(harness: HarnessChoice | null | undefined): PermissionPreset[] {
  return harness === "claude" ? CLAUDE_PERMISSION_PRESETS : PERMISSION_PRESETS;
}

const STORAGE_KEY = "pingex-composer-prefs";
const LEGACY_STORAGE_KEY = "pingu-composer-prefs";
/** Cap on remembered threads, so the store can't grow without bound. */
const MAX_THREADS = 300;

/**
 * Prefs are scoped: a thread keeps whatever it was last run with, a new thread
 * starts from the project's last-used choices, and a project nobody has touched
 * falls back to `fallback` (the pre-scoping global value, migrated once) and
 * then to hard defaults.
 */
interface PrefsStore {
  version: 2;
  fallback: ComposerPrefs | null;
  /**
   * Last-used subagent choices, applied to any scope that has never stored its
   * own. Model/effort/permission deliberately do not carry across projects;
   * the subagent policy is a workflow preference, so it does.
   */
  subagentDefaults: SubagentPrefs | null;
  projects: Record<string, ComposerPrefs>;
  threads: Record<string, ComposerPrefs>;
}

const defaults = (): ComposerPrefs => ({
  harness: null,
  model: null,
  effort: null,
  permissionPreset: null,
  planMode: false,
  appSubagents: null,
  subagentModelPolicy: null,
  subagentReasoningEffortPolicy: null,
});

const subagentDefaults = (): SubagentPrefs => ({
  appSubagents: null,
  subagentModelPolicy: null,
  subagentReasoningEffortPolicy: null,
});

/** Just the globally-carried fields, snapshotted off a full prefs object. */
function pickSubagent(prefs: SubagentPrefs): SubagentPrefs {
  return {
    appSubagents: prefs.appSubagents,
    subagentModelPolicy: prefs.subagentModelPolicy,
    subagentReasoningEffortPolicy: prefs.subagentReasoningEffortPolicy,
  };
}

function coerceSubagent(value: unknown): SubagentPrefs | null {
  if (!value || typeof value !== "object") return null;
  return { ...subagentDefaults(), ...(value as Partial<SubagentPrefs>) };
}

/**
 * Whether a thread started with these prefs should get the app's agent tools.
 * The per-thread choice wins; `null` defers to the global setting.
 */
export function resolveAppSubagents(prefs: ComposerPrefs, global: boolean): boolean {
  return prefs.appSubagents ?? global;
}

const emptyStore = (): PrefsStore => ({
  version: 2,
  fallback: null,
  subagentDefaults: null,
  projects: {},
  threads: {},
});

/** Drop unknown fields and fill in ones added since the entry was written. */
function coerce(value: unknown): ComposerPrefs | null {
  if (!value || typeof value !== "object") return null;
  return { ...defaults(), ...(value as Partial<ComposerPrefs>) };
}

function coerceScope(value: unknown): Record<string, ComposerPrefs> {
  const out: Record<string, ComposerPrefs> = {};
  if (!value || typeof value !== "object") return out;
  for (const [key, entry] of Object.entries(value as Record<string, unknown>)) {
    const prefs = coerce(entry);
    if (prefs) out[key] = prefs;
  }
  return out;
}

function readStore(): PrefsStore {
  try {
    const raw = localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!raw) return emptyStore();
    const parsed = JSON.parse(raw);
    if (parsed?.version === 2) {
      return {
        version: 2,
        fallback: coerce(parsed.fallback),
        subagentDefaults: coerceSubagent(parsed.subagentDefaults),
        projects: coerceScope(parsed.projects),
        threads: coerceScope(parsed.threads),
      };
    }
    // Pre-scoping: one flat blob of global choices. Keep it as the fallback so
    // nobody's current model selection disappears on upgrade.
    const migrated: PrefsStore = { ...emptyStore(), fallback: coerce(parsed) };
    writeStore(migrated);
    return migrated;
  } catch {
    return emptyStore();
  }
}

function writeStore(store: PrefsStore): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(store));
  } catch {
    // Persistence is best-effort; selections still apply for the session.
  }
}

/** Re-insert `key` so object key order stays newest-last, then trim the oldest. */
function touch(scope: Record<string, ComposerPrefs>, key: string, prefs: ComposerPrefs, max?: number): void {
  delete scope[key];
  scope[key] = prefs;
  if (!max) return;
  const keys = Object.keys(scope);
  for (const stale of keys.slice(0, Math.max(0, keys.length - max))) delete scope[stale];
}

/**
 * Prefs for a composer: this thread's, else this project's last-used, else
 * defaults. A scope that has never been written inherits the global subagent
 * choices, so configuring subagents once applies everywhere until a scope
 * disagrees.
 */
export function loadScopedPrefs(project: string, threadId: string | null): ComposerPrefs {
  const store = readStore();
  const own = (threadId ? store.threads[threadId] : null) ?? store.projects[project];
  const scoped = own ?? store.fallback;
  const prefs = scoped ? { ...scoped } : defaults();
  if (!own && store.subagentDefaults) Object.assign(prefs, store.subagentDefaults);
  return prefs;
}

/** Remember `prefs` for this thread (if any) and as the project's last-used. */
export function saveScopedPrefs(project: string, threadId: string | null, prefs: ComposerPrefs): void {
  const store = readStore();
  const snapshot = { ...prefs };
  if (project) touch(store.projects, project, snapshot);
  if (threadId) touch(store.threads, threadId, snapshot, MAX_THREADS);
  store.subagentDefaults = pickSubagent(snapshot);
  writeStore(store);
}

/** The global last-used choices, for composers with no project or thread. */
export function loadPrefs(): ComposerPrefs {
  const store = readStore();
  const prefs = store.fallback ? { ...store.fallback } : defaults();
  if (store.subagentDefaults) Object.assign(prefs, store.subagentDefaults);
  return prefs;
}

export function savePrefs(prefs: ComposerPrefs): void {
  const store = readStore();
  store.fallback = { ...prefs };
  store.subagentDefaults = pickSubagent(prefs);
  writeStore(store);
}

/** Whether `value` is permitted by `policy`; a null policy permits everything. */
export function policyAllows(policy: SubagentPolicy | null, value: string): boolean {
  if (!policy) return true;
  return "allowed" in policy ? policy.allowed.includes(value) : !policy.excluded.includes(value);
}

/**
 * Whether `policy` permits nothing out of `all` — a state no subagent could be
 * spawned under, so the composer refuses to send while it holds.
 */
export function policyIsEmpty(policy: SubagentPolicy | null, all: string[]): boolean {
  if (!policy) return false;
  if ("allowed" in policy) return policy.allowed.length === 0;
  return all.length > 0 && all.every((entry) => policy.excluded.includes(entry));
}

/** Turn the saved prefs into per-turn overrides for turn/start.
 *
 * `defaultModel` (Codex's default from the model list) and `threadModel` (what
 * the thread last ran on) back the collaboration settings when no model is
 * explicitly selected — the protocol requires a concrete model there. */
export function turnOptionsFrom(
  prefs: ComposerPrefs,
  subagentModelPolicy?: SubagentPolicy | null,
  subagentReasoningEffortPolicy?: SubagentPolicy | null,
  defaultModel?: string | null,
  threadModel?: string | null,
): TurnOptions | undefined {
  const preset = PERMISSION_PRESETS.find((candidate) => candidate.id === prefs.permissionPreset);
  const options: TurnOptions = {};
  if (prefs.model) options.model = prefs.model;
  if (prefs.effort) options.effort = prefs.effort;
  if (preset) {
    options.approvalPolicy = preset.approvalPolicy;
    options.sandboxMode = preset.sandboxMode;
  }
  if (subagentModelPolicy) options.subagentModelPolicy = subagentModelPolicy;
  if (subagentReasoningEffortPolicy) options.subagentReasoningEffortPolicy = subagentReasoningEffortPolicy;
  // Collaboration mode is a sticky thread setting on the Codex side, so the
  // thread only leaves plan mode when a turn explicitly sends "default".
  // Codex's CollaborationMode requires full settings; without a resolvable
  // model we skip the override rather than send an invalid request. Skipping is
  // a last resort: a turn without an explicit mode leaves Codex's mode diff in
  // a state where it never re-sends the "Default mode" instructions, so a
  // thread that was in plan mode looks stuck there to the model.
  const model = prefs.model ?? defaultModel ?? threadModel ?? null;
  if (model) {
    options.collaborationMode = {
      mode: prefs.planMode ? "plan" : "default",
      settings: { model, reasoning_effort: prefs.effort ?? null, developer_instructions: null },
    };
  }
  return Object.keys(options).length > 0 ? options : undefined;
}

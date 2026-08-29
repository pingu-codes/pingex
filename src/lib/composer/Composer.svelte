<script lang="ts">
import { ArrowUp, Bot, Map as MapIcon, Paperclip, Square } from "@lucide/svelte";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { onDestroy, onMount, untrack } from "svelte";
import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import ContextMeter from "$lib/composer/ContextMeter.svelte";
import {
  type AttachmentPart,
  buildTurnInput,
  type ComposerPart,
  type DetectedQueries,
  EMPTY_PARTS,
  hasSendableContent,
  normaliseParts,
} from "$lib/composer/composerParts";
import {
  type ComposerPrefs,
  type HarnessChoice,
  loadScopedPrefs,
  policyIsEmpty,
  saveScopedPrefs,
  turnOptionsFrom,
} from "$lib/composer/composerPrefs.svelte";
import MentionPicker from "$lib/composer/MentionPicker.svelte";
import ModelPopover from "$lib/composer/ModelPopover.svelte";
import {
  claudeModels as claudeModelList,
  claudeModelsError as claudeModelListError,
  ensureClaudeModels,
  ensureModels,
  models as modelList,
  modelsError as modelListError,
} from "$lib/composer/models.svelte";
import PermissionsPopover from "$lib/composer/PermissionsPopover.svelte";
import ReviewTargetPicker from "$lib/composer/ReviewTargetPicker.svelte";
import { RichEditor } from "$lib/composer/richEditor.svelte";
import SkillPicker from "$lib/composer/SkillPicker.svelte";
import SlashCommandPicker from "$lib/composer/SlashCommandPicker.svelte";
import SubagentPolicyPopover from "$lib/composer/SubagentPolicyPopover.svelte";
import { skillLabel } from "$lib/composer/skills";
import { INIT_PROMPT, parseSlashCommand, type SlashCommand, type SlashCommandId } from "$lib/composer/slashCommands";
import {
  deleteDraft,
  isTauri,
  loadDraft,
  removeStagedAttachment,
  saveDraft,
  stageAttachment,
  stageBrowserFile,
  stageClipboardImage,
} from "$lib/services/api";
import { openSettings, settingsNav } from "$lib/services/settingsNav.svelte";
import type { ContextStats } from "$lib/thread/contextUsage";
import { freshPlanPrompt } from "$lib/thread/planHandoff";
import type { Model, ReviewTarget, SkillSummary, SubagentPolicy, TurnOptions, UserInputPart } from "$lib/types";
import { clickOutside } from "$lib/utils/clickOutside";
import { loadSize, resizeHandle } from "$lib/utils/resize";

let {
  busy = false,
  disabled = false,
  hasQuestions = false,
  plan = null,
  cwd = "",
  draftKey = "",
  projectKey = "",
  threadId = null,
  contextStats = null,
  compacting = false,
  subagentModelPolicy = null,
  subagentReasoningEffortPolicy = null,
  threadModel = null,
  history = [],
  threadHarness = null,
  onSend,
  onInterrupt,
  onCommand,
  onReview,
  onImplementFresh,
  onSubagentPolicyChange,
  onModelChange,
  onLiveSettingsChange,
}: {
  busy?: boolean;
  disabled?: boolean;
  /** Codex asked the user questions that are still unanswered. */
  hasQuestions?: boolean;
  /** The latest plan the user hasn't responded to yet, if any. */
  plan?: string | null;
  cwd?: string;
  /** Draft scope (project path, optionally thread-suffixed); unsent text persists under it. */
  draftKey?: string;
  /** Project the composer belongs to; model/effort/plan choices fall back to its last-used. */
  projectKey?: string;
  /** Thread the composer belongs to, or null for an unsent draft thread. */
  threadId?: string | null;
  /** Live context usage for the thread, or null before Codex has reported any. */
  contextStats?: ContextStats | null;
  /** A compaction turn is running. */
  compacting?: boolean;
  subagentModelPolicy?: SubagentPolicy | null;
  subagentReasoningEffortPolicy?: SubagentPolicy | null;
  onSend: (input: UserInputPart[], options?: TurnOptions) => void;
  onInterrupt: () => void;
  /** Thread-level slash commands (compact, new, fork, archive, rename). */
  /** `argument` is whatever followed the command name, e.g. `/undo 2`. `typed`
   *  is the line as submitted, so a handler that fails can put it back via
   *  `restoreText` rather than leaving the user with an empty composer. */
  onCommand?: (command: SlashCommandId, argument?: string, typed?: string) => void;
  /** A review target chosen in the picker `openReviewPicker()` opened. */
  onReview?: (target: ReviewTarget) => void;
  /** Run the plan in a fresh thread instead of this one; absent = no such option. */
  onImplementFresh?: (input: UserInputPart[], options?: TurnOptions) => void;
  onSubagentPolicyChange?: (modelPolicy: SubagentPolicy | null, effortPolicy: SubagentPolicy | null) => void;
  /** The model turns will actually run on — the picked one, else Codex's default. */
  onModelChange?: (modelId: string | null) => void;
  /** The user picked a model or effort; the owner decides whether a running
   *  turn can take it now (`turn/settings/update`) or from the next turn. */
  onLiveSettingsChange?: (settings: { model: string | null; effort: string | null }) => void;
  /** The model the thread last ran on; backs the collaboration-mode settings
   *  when nothing is picked and the model list has not loaded yet. */
  threadModel?: string | null;
  /** Prior user messages, oldest first, for ↑/↓ recall from the composer's edges. */
  history?: string[];
  /** The harness an existing thread runs on. A draft reads the pref instead. */
  threadHarness?: HarnessChoice | null;
} = $props();

let editor = $state<HTMLDivElement | null>(null);
let composerBox = $state<HTMLDivElement | null>(null);

// The editor proper: parts, caret, undo and trigger queries live in the
// controller; this component renders it, wires its DOM events and supplies
// the decisions it does not own (submit, history, pickers, staging).
const richEditor: RichEditor = new RichEditor({
  cwd: () => cwd,
  chipHandlers: {
    onRetry: (id) => {
      const source = sources.get(id);
      if (source) void stageSource(id, source, false);
    },
    thumbSrc: (part) => thumbSrc(part),
  },
  onChipRemoved: (chip) => {
    const attachmentId = chip.dataset.attachmentId;
    if (!attachmentId) return;
    void removeStagedAttachment(attachmentId).catch(() => {});
    sources.delete(attachmentId);
  },
  pickerActive: () => pickerActive,
  pickerOpen: () => pickerOpen,
  suppressQueries: () => reviewPicker,
  onSubmit: () => submit(),
  onHistory: (direction) => recallHistory(direction),
  onEdit: () => {
    historyIndex = null;
  },
});

$effect(() => {
  if (editor) return richEditor.attach(editor);
});

// ↑/↓ at the composer's first/last line walk back through `history`, the way
// a shell does; the unsent draft is stashed on the way in and restored once
// the walk runs off the newest end. Any edit (the editor reports them through
// `onEdit`) ends the walk.
let historyIndex: number | null = null;
let historyDraft: ComposerPart[] | null = null;

function recallHistory(direction: "older" | "newer"): boolean {
  const entries = history ?? [];
  if (!richEditor.parts.every((part) => part.type === "text")) return false;
  if (direction === "older") {
    if (!richEditor.caretOnEdgeLine("first")) return false;
    const next = (historyIndex ?? entries.length) - 1;
    if (next < 0) return entries.length !== 0;
    if (historyIndex === null) historyDraft = $state.snapshot(richEditor.parts) as ComposerPart[];
    richEditor.setText(entries[next] ?? "");
    historyIndex = next;
    return true;
  }
  if (historyIndex === null || !richEditor.caretOnEdgeLine("last")) return false;
  const next = historyIndex + 1;
  if (next >= entries.length) {
    const draft = historyDraft ?? EMPTY_PARTS();
    historyDraft = null;
    richEditor.setParts(draft);
    historyIndex = null;
    return true;
  }
  richEditor.setText(entries[next] ?? "");
  historyIndex = next;
  return true;
}

// Per-project draft persistence: unsent input is saved (debounced) under the
// project's draft folder and restored the next time this project's composer
// mounts. `lastSavedDraft` is the serialized form already on disk (null =
// no draft), so unchanged content never rewrites it.
let draftLoaded = $state(false);
let lastSavedDraft: string | null = null;

function serializeDraft(): string | null {
  // Only ready attachments are worth persisting; a staging/failed one has no
  // usable staged path to restore.
  const persistable = richEditor.parts.filter((part) => part.type !== "attachment" || part.state === "ready");
  const normalised = normaliseParts(persistable.map((part) => ({ ...part })));
  return hasSendableContent(normalised) ? JSON.stringify(normalised) : null;
}

function persistDraft(serialized: string | null) {
  if (!draftKey || serialized === lastSavedDraft) return;
  lastSavedDraft = serialized;
  if (serialized === null) void deleteDraft(draftKey);
  else void saveDraft(draftKey, serialized);
}

onMount(() => {
  let cancelled = false;
  (async () => {
    if (draftKey) {
      try {
        const stored = await loadDraft(draftKey);
        const untouched = !richEditor.parts.some((part) => part.type !== "text" || part.text.length > 0);
        if (!cancelled && stored && untouched) {
          // The restored draft is the floor of the undo history, not an edit.
          richEditor.setParts(JSON.parse(stored) as ComposerPart[], null, { focus: false });
          richEditor.resetUndo();
          lastSavedDraft = stored;
        }
      } catch {
        // An unreadable or corrupt draft should never block typing.
      }
    }
    if (!cancelled) draftLoaded = true;
  })();
  return () => {
    cancelled = true;
  };
});

// A draft thread being adopted re-keys the composer without remounting. The
// editor content follows the user into the new thread, so don't reload — just
// forget what was written under the old key so the next save isn't skipped.
let draftScope = untrack(() => draftKey);
$effect(() => {
  if (draftKey === draftScope) return;
  draftScope = draftKey;
  lastSavedDraft = null;
});

$effect(() => {
  const serialized = serializeDraft();
  if (!draftLoaded || !draftKey || serialized === lastSavedDraft) return;
  const timer = setTimeout(() => persistDraft(serialized), 400);
  return () => clearTimeout(timer);
});

// The thread view is torn down on every navigation; flush what the debounce
// hasn't written yet so nothing typed is lost.
onDestroy(() => {
  if (draftLoaded) persistDraft(serializeDraft());
});

// Native (Tauri) file drag/drop. The webview delivers OS file drops with real
// paths here (HTML drag events are suppressed under Tauri); highlight only when
// the cursor is over the composer, and stage the dropped paths.
onMount(() => {
  if (!isTauri()) return;
  let unlisten: (() => void) | undefined;
  let cancelled = false;
  const overComposer = (position: { x: number; y: number }) => {
    const box = composerBox?.getBoundingClientRect();
    if (!box) return false;
    const ratio = window.devicePixelRatio || 1;
    const x = position.x / ratio;
    const y = position.y / ratio;
    return x >= box.left && x <= box.right && y >= box.top && y <= box.bottom;
  };
  void getCurrentWebview()
    .onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === "enter" || payload.type === "over") {
        dragOver = overComposer(payload.position);
      } else if (payload.type === "drop") {
        if (overComposer(payload.position) && !disabled) attachPaths(payload.paths);
        dragOver = false;
      } else {
        dragOver = false;
      }
    })
    .then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    })
    .catch(() => {});
  return () => {
    cancelled = true;
    unlisten?.();
  };
});

// Draggable minimum height of the input; typing auto-grows it up to MAX_INPUT_HEIGHT.
const MAX_INPUT_HEIGHT = 360;
let inputHeight = $state(loadSize("layout.composerHeight", 60, 36, MAX_INPUT_HEIGHT));

function autogrow() {
  const element = editor;
  if (!element) return;
  // Collapsing to "auto" to measure resets scrollTop; put it back afterwards
  // (the browser clamps it if the content shrank).
  const scrollTop = element.scrollTop;
  element.style.height = "auto";
  element.style.height = `${Math.min(Math.max(element.scrollHeight, inputHeight), MAX_INPUT_HEIGHT)}px`;
  element.scrollTop = scrollTop;
}

$effect(() => {
  void richEditor.parts;
  void inputHeight;
  autogrow();
});

// Model/effort/permissions/plan mode are remembered per thread, seeded from the
// project's last-used choices. `prefsScope` is the scope those were loaded for,
// so a scope change mid-mount (a draft thread being adopted) is detectable.
let prefs = $state<ComposerPrefs>(untrack(() => loadScopedPrefs(projectKey, threadId)));
let prefsScope = untrack(() => threadId);

$effect(() => {
  if (threadId === prefsScope) return;
  // A draft thread that just became a real one keeps the choices the user made
  // while drafting; any other scope change loads that scope's own prefs.
  if (prefsScope === null && threadId) saveScopedPrefs(projectKey, threadId, prefs);
  else prefs = loadScopedPrefs(projectKey, threadId);
  prefsScope = threadId;
});

/** Hand the remembered subagent policy up to the thread. */
function seedSubagentPolicy() {
  onSubagentPolicyChange?.(prefs.subagentModelPolicy, prefs.subagentReasoningEffortPolicy);
}

/**
 * Reconcile the thread's subagent policy with the remembered one. A thread that
 * carries its own policy wins — that is what Codex will actually enforce — and
 * a thread with none adopts the remembered choices, which is how a new thread
 * inherits the last-used ones.
 */
$effect(() => {
  void threadId;
  const model = subagentModelPolicy;
  const effort = subagentReasoningEffortPolicy;
  untrack(() => {
    let seed = false;
    if (model !== null) prefs.subagentModelPolicy = model;
    else seed ||= prefs.subagentModelPolicy !== null;
    if (effort !== null) prefs.subagentReasoningEffortPolicy = effort;
    else seed ||= prefs.subagentReasoningEffortPolicy !== null;
    if (seed) seedSubagentPolicy();
  });
});
/** Which harness this composer is talking to: the thread's own, else the
 *  draft's chosen one, else Codex. */
const harness = $derived<HarnessChoice>(threadHarness ?? prefs.harness ?? "codex");
const models = $derived(harness === "claude" ? claudeModelList() : modelList());
const modelsError = $derived(harness === "claude" ? claudeModelListError() : modelListError());

function ensureHarnessModels(): Promise<void> {
  return harness === "claude" ? ensureClaudeModels() : ensureModels();
}
let popover = $state<"model" | "subagents" | "permissions" | null>(null);

// The @-mention, /-command and $-skill pickers are driven by the trigger
// queries the editor detects (`richEditor.queries`).
const queries: DetectedQueries = $derived(richEditor.queries);

// `/review` target picker state. Unlike the three above it is not driven by a
// trigger character, so it opens on request and filters on the whole line.
let reviewPicker = $state(false);
const reviewQuery = $derived(
  richEditor.parts.length === 1 && richEditor.parts[0].type === "text" ? richEditor.parts[0].text : "",
);

/** Whether any picker is open. An open picker always owns Escape. */
const pickerOpen: boolean = $derived(
  queries.mentionQuery !== null || queries.slashQuery !== null || queries.skillQuery !== null || reviewPicker,
);
/**
 * Results in the open picker. Arrow/Enter/Tab only belong to it while it has
 * something to pick — otherwise typing `/notacommand` or `@nothingmatches` and
 * pressing Enter would be swallowed and the message could never be sent.
 */
let pickerCount = $state(0);
const pickerActive = $derived(pickerOpen && pickerCount > 0);

const selectedModel = $derived((models ?? []).find((model) => model.id === prefs.model) ?? null);
const effortOptions = $derived(
  (selectedModel ?? (models ?? []).find((model) => model.isDefault) ?? null)?.supportedReasoningEfforts ?? [],
);
const subagentEfforts = $derived([
  ...new Set(
    (models ?? []).flatMap((model) => model.supportedReasoningEfforts.map((option) => option.reasoningEffort)),
  ),
]);
const hasSendable = $derived(hasSendableContent(richEditor.parts));

/**
 * Why this composer refuses to send, or null when it will. A turn carries the
 * model, permission preset and subagent policy with it, so a setting that
 * cannot produce a valid turn has to be fixed before the message goes out
 * rather than silently resolving to something the user did not choose.
 */
const invalidReason = $derived.by(() => {
  if (
    policyIsEmpty(
      subagentModelPolicy,
      (models ?? []).map((model) => model.id),
    )
  ) {
    return "Pick at least one subagent model";
  }
  if (policyIsEmpty(subagentReasoningEffortPolicy, subagentEfforts)) {
    return "Pick at least one subagent effort level";
  }
  if (!prefs.model) return "Choose a model";
  if (!prefs.permissionPreset) return "Choose a permission mode";
  return null;
});

const modelButtonLabel = $derived.by(() => {
  const name = selectedModel?.displayName ?? "Model";
  return prefs.effort ? `${name} · ${prefs.effort}` : name;
});

function persist() {
  saveScopedPrefs(projectKey, threadId, prefs);
}

const defaultModel = $derived((models ?? []).find((model) => model.isDefault) ?? null);
const effectiveModel = $derived(prefs.model ?? defaultModel?.id ?? null);
/** The effort a turn runs at, including the model default the user never touched. */
const effectiveEffort = $derived(prefs.effort ?? (selectedModel ?? defaultModel)?.defaultReasoningEffort ?? null);
$effect(() => {
  onModelChange?.(effectiveModel);
});

/** Turn overrides plus the resolved pair the transcript labels replies with. */
function sendOptions(): TurnOptions | undefined {
  const options = turnOptionsFrom(
    prefs,
    subagentModelPolicy,
    subagentReasoningEffortPolicy,
    defaultModel?.id ?? null,
    threadModel,
  );
  if (!options?.collaborationMode) {
    console.warn("composer: no model resolved; turn sent without an explicit collaboration mode");
  }
  if (!effectiveModel && !effectiveEffort) return options;
  return { ...options, resolvedModel: effectiveModel, resolvedEffort: effectiveEffort };
}

async function togglePopover(which: "model" | "subagents" | "permissions") {
  popover = popover === which ? null : which;
  if (popover === "model" || popover === "subagents") await ensureHarnessModels();
}

// Every turn sends an explicit collaboration mode (plan or default), and its
// settings need a concrete model, so make sure the model list is available
// before the first send.
$effect(() => {
  void ensureHarnessModels();
});

function chooseModel(model: Model) {
  prefs.model = model.id;
  if (!model.supportedReasoningEfforts.some((option) => option.reasoningEffort === prefs.effort)) {
    prefs.effort = model.defaultReasoningEffort;
  }
  persist();
  onLiveSettingsChange?.({ model: prefs.model ?? null, effort: prefs.effort ?? null });
}

function chooseEffort(effort: string) {
  prefs.effort = effort;
  persist();
  onLiveSettingsChange?.({ model: prefs.model ?? null, effort: prefs.effort ?? null });
}

function choosePermission(id: string) {
  // No un-toggling back to "unset": a turn needs a preset, and sending is
  // blocked without one.
  prefs.permissionPreset = id;
  persist();
  popover = null;
}

/** Apply a subagent policy change to the thread and remember it for the next one. */
function setSubagentPolicy(modelPolicy: SubagentPolicy | null, effortPolicy: SubagentPolicy | null) {
  prefs.subagentModelPolicy = modelPolicy;
  prefs.subagentReasoningEffortPolicy = effortPolicy;
  persist();
  onSubagentPolicyChange?.(modelPolicy, effortPolicy);
}

function toggleSubagentModel(id: string) {
  const all = (models ?? []).map((model) => model.id);
  const current = subagentModelPolicy
    ? "allowed" in subagentModelPolicy
      ? subagentModelPolicy.allowed
      : all.filter((entry) => !subagentModelPolicy.excluded.includes(entry))
    : all;
  const allowed = current.includes(id) ? current.filter((entry) => entry !== id) : [...current, id];
  setSubagentPolicy(allowed.length === all.length ? null : { allowed }, subagentReasoningEffortPolicy);
}

function toggleSubagentEffort(effort: string) {
  const current = subagentReasoningEffortPolicy
    ? "allowed" in subagentReasoningEffortPolicy
      ? subagentReasoningEffortPolicy.allowed
      : subagentEfforts.filter((entry) => !subagentReasoningEffortPolicy.excluded.includes(entry))
    : subagentEfforts;
  const allowed = current.includes(effort) ? current.filter((entry) => entry !== effort) : [...current, effort];
  setSubagentPolicy(subagentModelPolicy, allowed.length === subagentEfforts.length ? null : { allowed });
}

function setAppSubagents(value: boolean | null) {
  prefs.appSubagents = value;
  persist();
}

function togglePlanMode() {
  prefs.planMode = !prefs.planMode;
  persist();
}

// The plan the user already acted on ("Keep planning" or "Implement the
// plan"), so the action bar only reappears when a newer plan arrives.
let dismissedPlan = $state<string | null>(null);
const showPlanActions = $derived(
  prefs.planMode &&
    plan !== null &&
    plan !== dismissedPlan &&
    !busy &&
    !disabled &&
    !hasQuestions &&
    // These buttons call onSend directly, so they have to honour the same guard.
    !invalidReason,
);

/** Leaving plan mode to act on `plan`: the action bar only returns for a newer one. */
function leavePlanMode() {
  dismissedPlan = plan;
  prefs.planMode = false;
  persist();
}

function planTurnOptions() {
  return sendOptions();
}

/**
 * Whether a thread started from this composer should get the app's agent
 * tools: the per-thread choice, or `null` to follow the global setting.
 *
 * Read by ThreadView at `thread/start` time, because `dynamicTools` is only
 * accepted there — an existing thread cannot be switched over.
 */
export function appSubagentsChoice(): boolean | null {
  return prefs.appSubagents;
}

/**
 * Which harness a draft starts on. Read by ThreadView at thread creation;
 * an existing thread never switches.
 */
export function harnessChoice(): HarnessChoice | null {
  return prefs.harness;
}

/** Codex ids and Claude aliases do not overlap, so a switch drops the
 *  picked model and effort and lets the other harness's defaults apply. */
function toggleHarness() {
  prefs.harness = harness === "claude" ? "codex" : "claude";
  prefs.model = null;
  prefs.effort = null;
  persist();
}

export function implementPlan() {
  leavePlanMode();
  onSend([{ type: "text", text: "Implement the plan." }], planTurnOptions());
}

/** Implement the plan in a new thread, carrying the plan across as its only
 *  context — the planning conversation itself is left behind. */
export function implementPlanFresh(planText: string | null = plan) {
  if (!planText || !onImplementFresh) return;
  leavePlanMode();
  onImplementFresh([{ type: "text", text: freshPlanPrompt(planText) }], planTurnOptions());
}

function keepPlanning() {
  dismissedPlan = plan;
  editor?.focus();
}

/** Empties the editor, leaving the caret in it. */
function clearText() {
  richEditor.setParts(EMPTY_PARTS(), 0);
}

/**
 * Offer the `/review` targets. Called by the thread view rather than from here:
 * it owns the thread, so it is the one that knows a review can start at all.
 */
export function openReviewPicker() {
  clearText();
  richEditor.closeQueries();
  reviewPicker = true;
}

function pickReviewTarget(target: ReviewTarget) {
  reviewPicker = false;
  clearText();
  onReview?.(target);
}

function runCommand(command: SlashCommand, argument = "") {
  const typed = argument ? `/${command.id} ${argument}` : `/${command.id}`;
  clearText();
  richEditor.closeQueries();
  if (command.id === "plan") {
    togglePlanMode();
  } else if (command.id === "model" || command.id === "permissions") {
    togglePopover(command.id);
  } else if (command.id === "init") {
    // `/init` is a prompt, not an action: put it in the composer so the user
    // can adjust the wording before sending.
    richEditor.setText(INIT_PROMPT);
  } else if (command.scope === "settings") {
    openSettings("integrations");
  } else {
    onCommand?.(command.id, argument, typed);
  }
}

/**
 * Put a submitted command back in the composer. `runCommand` clears the line
 * optimistically; a handler that could not run the command calls this so the
 * text the user typed is not lost behind an error toast.
 */
export function restoreText(text: string) {
  richEditor.setText(text);
}

/** True when nothing sendable has been typed or attached. */
export function isEmpty(): boolean {
  return !hasSendableContent(normaliseParts(richEditor.parts.map((part) => ({ ...part }))));
}

/**
 * Dismisses whichever picker is open. The editor's own re-detection can't do
 * this: it bails when the caret is outside the editor, so losing focus would
 * otherwise leave the popup stranded on screen.
 */
function closePickers() {
  richEditor.closeQueries();
  reviewPicker = false;
}

function insertSkill(skill: SkillSummary) {
  richEditor.insertSkill(skill.name, skill.path, skillLabel(skill));
}

// --- Attachments ---
// Files/images enter through the paperclip, drag/drop, or an image paste. Each
// gets an inline chip that shows staging → ready/failed; `sources` keeps what
// is needed to Retry a failed one, keyed by the chip's current id.
type StageSource =
  | { via: "path"; path: string }
  | { via: "file"; file: File }
  | { via: "bytes"; filename: string; mime: string; bytes: number[] };

const sources = new Map<string, StageSource>();
let dragOver = $state(false);

const IMAGE_EXTENSIONS = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "heic", "svg"];
const basename = (path: string) => path.split(/[/\\]/).pop() || path;
function guessKind(filename: string, mime = ""): "image" | "file" {
  if (mime.startsWith("image/")) return "image";
  const dot = filename.lastIndexOf(".");
  return dot >= 0 && IMAGE_EXTENSIONS.includes(filename.slice(dot + 1).toLowerCase()) ? "image" : "file";
}

function placeholderFor(clientId: string, source: StageSource): AttachmentPart {
  const filename =
    source.via === "path" ? basename(source.path) : source.via === "file" ? source.file.name : source.filename;
  const mime = source.via === "bytes" ? source.mime : source.via === "file" ? source.file.type : "";
  const size = source.via === "file" ? source.file.size : source.via === "bytes" ? source.bytes.length : 0;
  return {
    type: "attachment",
    id: clientId,
    filename,
    mime,
    size,
    path: "",
    kind: guessKind(filename, mime),
    state: "staging",
  };
}

const thumbSrc = (part: AttachmentPart): string | null => {
  if (part.kind !== "image" || !part.path) return null;
  return isTauri() ? convertFileSrc(part.path) : part.path;
};

let nextClientId = 0;

/** Insert (or, on retry, refresh) an attachment chip and run its staging. */
async function stageSource(clientId: string, source: StageSource, insert: boolean) {
  if (!editor) return;
  sources.set(clientId, source);
  const placeholder = placeholderFor(clientId, source);
  if (insert) richEditor.insertAttachment(placeholder);
  else richEditor.updateAttachment(clientId, placeholder);
  try {
    const staged =
      source.via === "path"
        ? await stageAttachment(source.path)
        : source.via === "file"
          ? await stageBrowserFile(source.file)
          : await stageClipboardImage(source.filename, source.mime, source.bytes);
    const ready: AttachmentPart = {
      type: "attachment",
      id: staged.id,
      filename: staged.filename,
      mime: staged.mime,
      size: staged.size,
      path: staged.stagedPath,
      kind: staged.kind as AttachmentPart["kind"],
      state: "ready",
    };
    richEditor.updateAttachment(clientId, ready);
    sources.delete(clientId);
    sources.set(staged.id, source);
  } catch {
    richEditor.updateAttachment(clientId, { ...placeholder, state: "failed" });
  }
}

function attachPaths(paths: string[]) {
  for (const path of paths) {
    void stageSource(`c${nextClientId++}`, { via: "path", path }, true);
  }
}

function attachFiles(files: File[]) {
  for (const file of files) {
    void stageSource(`c${nextClientId++}`, { via: "file", file }, true);
  }
}

/** Native (Tauri) or browser file picker behind the paperclip button. */
async function pickFiles() {
  if (disabled) return;
  if (isTauri()) {
    const picked = await openFileDialog({ multiple: true });
    if (Array.isArray(picked)) attachPaths(picked);
    else if (typeof picked === "string") attachPaths([picked]);
    return;
  }
  fileInput?.click();
}

let fileInput = $state<HTMLInputElement | null>(null);

function onFileInputChange(event: Event) {
  const input = event.currentTarget as HTMLInputElement;
  if (input.files) attachFiles([...input.files]);
  input.value = "";
}

// Browser (Playwright) drag/drop: under Tauri these HTML events are suppressed
// and the native `onDragDropEvent` listener (registered in onMount) handles it.
function onDrop(event: DragEvent) {
  if (isTauri()) return;
  event.preventDefault();
  dragOver = false;
  const files = event.dataTransfer?.files;
  if (files?.length) attachFiles([...files]);
}

function onDragOver(event: DragEvent) {
  if (isTauri()) return;
  event.preventDefault();
  dragOver = true;
}

function onDragLeave() {
  if (isTauri()) return;
  dragOver = false;
}

/** Read a pasted/dropped image `File` into bytes for native staging. */
async function stageImageFile(file: File) {
  if (isTauri()) {
    const bytes = [...new Uint8Array(await file.arrayBuffer())];
    void stageSource(
      `c${nextClientId++}`,
      { via: "bytes", filename: file.name || "pasted-image.png", mime: file.type || "image/png", bytes },
      true,
    );
  } else {
    attachFiles([file]);
  }
}

function submit() {
  // Enter belongs to the review picker while it is open, even when its list is
  // empty — its filter text is not a message.
  if (reviewPicker) return;
  // A submitted `/command` runs rather than being sent as a message. The picker
  // has already closed by this point if an argument was typed, so this — not
  // the picker — is what makes `/review the auth changes` work.
  const parts = richEditor.parts;
  const typed = parts.length === 1 && parts[0].type === "text" ? parts[0].text : null;
  const command = typed !== null ? parseSlashCommand(typed) : null;
  if (command) {
    runCommand(command.command, command.argument);
    return;
  }
  const sentParts = parts.map((part) => ({ ...part }));
  const firstText = sentParts.find((part): part is Extract<ComposerPart, { type: "text" }> => part.type === "text");
  const lastText = [...sentParts]
    .reverse()
    .find((part): part is Extract<ComposerPart, { type: "text" }> => part.type === "text");
  if (firstText) firstText.text = firstText.text.trimStart();
  if (lastText) lastText.text = lastText.text.trimEnd();
  const trimmedParts = normaliseParts(sentParts);
  if (!hasSendableContent(trimmedParts) || disabled || hasQuestions || invalidReason) return;
  const sent = buildTurnInput(trimmedParts, cwd);
  richEditor.clear();
  sources.clear();
  closePickers();
  persistDraft(null);
  void dispatchSend(sent);
}

/** Send once a model can back the collaboration settings: the first send after
 *  launch may beat the model list, and a mode-less turn is what leaves a thread
 *  stuck in plan mode. Bounded so a failing model fetch never blocks sending. */
async function dispatchSend(sent: UserInputPart[]) {
  if (!models && !prefs.model && !threadModel) {
    await Promise.race([ensureModels(), new Promise((resolve) => setTimeout(resolve, 3000))]);
  }
  onSend(sent, sendOptions());
}

function onPaste(event: ClipboardEvent) {
  const items = [...(event.clipboardData?.items ?? [])];
  const imageItem = items.find((item) => item.kind === "file" && item.type.startsWith("image/"));
  if (imageItem) {
    const file = imageItem.getAsFile();
    if (file) {
      event.preventDefault();
      void stageImageFile(file);
      return;
    }
  }
  event.preventDefault();
  document.execCommand("insertText", false, event.clipboardData?.getData("text/plain") ?? "");
}
</script>

<svelte:window
  onclick={() => (popover = null)}
  onkeydown={(event) => {
    if (event.key !== "Escape") return;
    // The Settings overlay owns Escape while open (it closes the panel); an
    // Escape there must not reach through and interrupt the running turn.
    if (settingsNav.open) return;
    if (popover !== null) {
      popover = null;
      return;
    }
    // The mention/slash/skill pickers own Escape while open.
    if (pickerOpen) return;
    // While questions are up, Escape must not nuke the turn mid-answer; use Stop.
    if (busy && !hasQuestions) onInterrupt();
  }}
/>

<div class="relative border-t border-surface-200-800 bg-surface-50-950 px-6 py-3">
  <div
    role="separator"
    aria-orientation="horizontal"
    aria-label="Resize message input"
    class="absolute inset-x-0 -top-1 z-20 h-2 cursor-row-resize transition-colors hover:bg-primary-500/30 active:bg-primary-500/40"
    use:resizeHandle={{
      axis: "y",
      direction: -1,
      min: 36,
      max: MAX_INPUT_HEIGHT,
      storageKey: "layout.composerHeight",
      getSize: () => inputHeight,
      onResize: (size) => (inputHeight = size),
    }}
  ></div>
  <!--
    This wrapper is both the pickers' positioning parent and the composer box's
    container, so one boundary covers "pressed a picker row" and "clicked back
    in the input" as inside.
  -->
  <div
    class="relative mx-auto max-w-3xl"
    use:clickOutside={() => {
      if (pickerOpen) closePickers();
    }}
  >
    {#if showPlanActions}
      <div class="mb-2 flex items-center gap-2 rounded-xl border border-surface-200-800 bg-surface-100-900 px-3 py-2">
        <span class="min-w-0 flex-1 truncate text-xs text-surface-500">Plan ready — implement it, or keep refining in plan mode.</span>
        <button
          onclick={keepPlanning}
          class="shrink-0 rounded-full px-2.5 py-1 text-[11px] text-surface-600-400 transition hover:bg-surface-200-800 hover:text-surface-800-200"
        >
          Keep planning
        </button>
        {#if onImplementFresh}
          <TooltipButton
            label="Start a new thread whose only context is the plan"
            aria-label="Clear context and implement the plan"
            onclick={() => implementPlanFresh()}
            class="shrink-0 rounded-full px-2.5 py-1 text-[11px] text-surface-600-400 transition hover:bg-surface-200-800 hover:text-surface-800-200"
          >
            Clear context & implement
          </TooltipButton>
        {/if}
        <button
          onclick={implementPlan}
          class="shrink-0 rounded-full px-2.5 py-1 text-[11px] preset-filled-primary-500"
        >
          Implement the plan
        </button>
      </div>
    {/if}
    {#if reviewPicker}
      <ReviewTargetPicker
        {cwd}
        query={reviewQuery}
        scope={editor}
        onPick={pickReviewTarget}
        onClose={() => (reviewPicker = false)}
        onCount={(count) => (pickerCount = count)}
        onStageChange={clearText}
      />
    {:else if queries.mentionQuery !== null}
      <MentionPicker
        {cwd}
        query={queries.mentionQuery}
        scope={editor}
        onPick={(mention) => richEditor.insertMention(mention)}
        onClose={() => richEditor.closeQueries()}
        onCount={(count) => (pickerCount = count)}
      />
    {:else if queries.skillQuery !== null}
      <SkillPicker
        {cwd}
        query={queries.skillQuery}
        scope={editor}
        onPick={insertSkill}
        onClose={() => richEditor.closeQueries()}
        onCount={(count) => (pickerCount = count)}
      />
    {:else if queries.slashQuery !== null}
      <SlashCommandPicker
        query={queries.slashQuery}
        scope={editor}
        onPick={(command) => runCommand(command)}
        onClose={() => richEditor.closeQueries()}
        onCount={(count) => (pickerCount = count)}
      />
    {/if}

    <div
      bind:this={composerBox}
      ondrop={onDrop}
      ondragover={onDragOver}
      ondragenter={onDragOver}
      ondragleave={onDragLeave}
      role="group"
      aria-label="Message composer"
      class="rounded-2xl border bg-surface-100-900 px-3 py-2 transition-colors {dragOver
        ? 'border-primary-500 bg-primary-500/5'
        : 'border-surface-200-800 focus-within:border-surface-400-600'}"
    >
      {#if dragOver}
        <div class="pointer-events-none mb-1 text-center text-[11px] font-medium text-primary-600-400">
          Drop files to attach
        </div>
      {/if}
      <input
        bind:this={fileInput}
        type="file"
        multiple
        class="hidden"
        aria-hidden="true"
        tabindex={-1}
        onchange={onFileInputChange}
      />
      <div class="flex items-end gap-2">
        <div
          bind:this={editor}
          contenteditable={!disabled}
          spellcheck="false"
          {...({ autocorrect: "off", autocapitalize: "off" } as Record<string, string>)}
          tabindex={disabled ? -1 : 0}
          role="textbox"
          aria-multiline="true"
          aria-label="Message Codex… (@ to attach files, / for commands)"
          data-placeholder="Message Codex… (@ to attach files, / for commands)"
          onkeydown={richEditor.handleKey}
          oninput={richEditor.handleInput}
          onclick={richEditor.handleClick}
          onpaste={onPaste}
          oncompositionstart={richEditor.handleCompositionStart}
          oncompositionend={richEditor.handleCompositionEnd}
          class="composer-editor flex-1 overflow-y-auto bg-transparent text-sm leading-6 outline-none empty:before:pointer-events-none empty:before:text-surface-500 empty:before:content-[attr(data-placeholder)] {disabled ? 'pointer-events-none opacity-50' : ''}"
        >
        </div>
        {#if busy}
          <TooltipButton
            label="Stop (Esc)"
            onclick={onInterrupt}
            aria-label="Stop"
            class="grid size-7 shrink-0 place-items-center rounded-full preset-filled-error-500"
          >
            <Square size={12} fill="currentColor" />
          </TooltipButton>
        {/if}
        <!-- Tooltip lives on the wrapper: WebKit does not show titles on disabled buttons. -->
        <TooltipAnchor
          label={invalidReason ?? (hasQuestions ? "Answer questions" : busy && hasSendable ? "Queue message — sends when Codex finishes" : "Send message")}
          class="shrink-0"
        >
          <button
            onclick={submit}
            aria-label={busy ? "Queue message" : "Send message"}
            disabled={disabled || !hasSendable || hasQuestions || Boolean(invalidReason)}
            class="grid size-7 place-items-center rounded-full preset-filled-primary-500 disabled:opacity-40"
          >
            <ArrowUp size={14} />
          </button>
        </TooltipAnchor>
      </div>

      <div class="mt-1.5 flex items-center gap-1.5">
        <TooltipButton
          label="Attach files"
          onclick={pickFiles}
          disabled={disabled}
          aria-label="Attach files"
          class="inline-flex items-center rounded-full px-1.5 py-1 text-surface-500 transition hover:bg-surface-200-800 hover:text-surface-800-200 disabled:opacity-40"
        >
          <Paperclip size={14} />
        </TooltipButton>
        {#if !threadId}
          <TooltipButton
            label={harness === "claude" ? "New threads run on Claude Code. Switch to Codex" : "New threads run on Codex. Switch to Claude Code"}
            onclick={toggleHarness}
            aria-label="Choose harness"
            class="inline-flex items-center gap-1.5 rounded-full px-2 py-1 text-[11px] transition {harness === 'claude' ? 'preset-tonal-primary' : 'text-surface-500 hover:bg-surface-200-800 hover:text-surface-800-200'}"
          >
            <Bot size={12} />
            {harness === "claude" ? "Claude" : "Codex"}
          </TooltipButton>
        {/if}
        <TooltipButton
          label={prefs.planMode ? "Plan mode on" : "Toggle plan mode"}
          onclick={togglePlanMode}
          aria-label="Toggle plan mode"
          aria-pressed={prefs.planMode}
          class="inline-flex items-center gap-1.5 rounded-full px-2 py-1 text-[11px] transition {prefs.planMode ? 'preset-filled-primary-500' : 'text-surface-500 hover:bg-surface-200-800 hover:text-surface-800-200'}"
        >
          <MapIcon size={12} />
          Plan
        </TooltipButton>
        <ModelPopover
          open={popover === "model"}
          {models}
          {modelsError}
          modelId={prefs.model}
          effort={prefs.effort}
          {effortOptions}
          label={modelButtonLabel}
          onToggle={() => togglePopover("model")}
          onChooseModel={chooseModel}
          onChooseEffort={chooseEffort}
        />
        <SubagentPolicyPopover
          open={popover === "subagents"}
          {models}
          {modelsError}
          efforts={subagentEfforts}
          modelPolicy={subagentModelPolicy}
          effortPolicy={subagentReasoningEffortPolicy}
          appSubagents={prefs.appSubagents}
          appSubagentsLocked={Boolean(threadId)}
          onToggle={() => togglePopover("subagents")}
          onToggleModel={toggleSubagentModel}
          onToggleEffort={toggleSubagentEffort}
          onSetAppSubagents={setAppSubagents}
        />
        <PermissionsPopover
          open={popover === "permissions"}
          selectedId={prefs.permissionPreset}
          onToggle={() => togglePopover("permissions")}
          harness={harness}
          onChoose={choosePermission}
        />
        <div class="ml-auto">
          <ContextMeter
            stats={contextStats}
            {compacting}
            busy={busy || disabled}
            onCompact={onCommand ? () => onCommand("compact") : undefined}
          />
        </div>
      </div>
    </div>
  </div>
</div>

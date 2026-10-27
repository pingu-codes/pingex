/**
 * The composer's turn settings as one object: the remembered prefs (model,
 * effort, permissions, plan mode, speed, subagents, harness), what they
 * resolve to against the harness's model list, and the turn options a send
 * carries. Construct it during component init: it registers effects that keep
 * the prefs in step with the thread.
 */

import { untrack } from "svelte";
import {
  type ComposerPrefs,
  type HarnessChoice,
  loadScopedPrefs,
  policyIsEmpty,
  saveScopedPrefs,
  togglePolicy,
  turnOptionsFrom,
} from "$lib/composer/composerPrefs";
import { fastModeState, fastTier } from "$lib/composer/fastMode";
import { isModeOf, isPlanMode, modeFromPrefs, modesFor, selectMode } from "$lib/composer/harnessModes";
import { ensureHarnessModels, harnessModels, harnessModelsError } from "$lib/composer/models.svelte";
import type { Model, SubagentPolicy, TurnOptions } from "$lib/types";
import { can } from "$lib/utils/capabilities";

/** What the settings read from the composer's props. Getters, so each one is
 *  tracked on its own. */
export interface ComposerSettingsDeps {
  projectKey: () => string;
  threadId: () => string | null;
  /** The harness an existing thread runs on. A draft reads the pref instead. */
  threadHarness: () => HarnessChoice | null;
  /** The model the thread last ran on. */
  threadModel: () => string | null;
  threadSpeedTier: () => string | null | undefined;
  runningSpeedTier: () => string | null | undefined;
  busy: () => boolean;
  /** The thread's subagent policies, which win over the remembered ones. */
  subagentModelPolicy: () => SubagentPolicy | null;
  subagentReasoningEffortPolicy: () => SubagentPolicy | null;
  onSubagentPolicyChange: (modelPolicy: SubagentPolicy | null, effortPolicy: SubagentPolicy | null) => void;
  /** The model turns will actually run on — the picked one, else the default. */
  onModelChange: (modelId: string | null) => void;
  /** The user picked a model, effort or speed for a possibly running turn. */
  onLiveSettingsChange: (settings: { model?: string | null; effort?: string | null; speedTier?: string }) => void;
}

export class ComposerSettings {
  /** Remembered per thread, seeded from the project's last-used choices
   *  (loaded in the constructor, once `deps` is there to scope them). */
  prefs = $state<ComposerPrefs>() as ComposerPrefs;

  /** Which harness this composer is talking to: the thread's own, else the
   *  draft's chosen one, else Codex. */
  // `$derived.by` wherever `deps` is read: it is assigned after the field
  // initialisers run, and only a function body defers the read for TypeScript.
  readonly harness = $derived.by<HarnessChoice>(() => this.deps.threadHarness() ?? this.prefs.harness ?? "codex");
  /** The harness's own mode, for a harness with permission modes. */
  readonly harnessMode = $derived(modeFromPrefs(this.prefs, this.harness));
  readonly modeOptions = $derived(modesFor(this.harness));
  /** The harness switches named permission modes itself (plan among them),
   *  rather than taking presets plus a per-turn collaboration mode. */
  readonly permissionModes = $derived(can(this.harness, "permissionModes"));
  /** Plan or default is sent as a collaboration mode with every turn. */
  readonly collaborationModes = $derived(can(this.harness, "collaborationModes"));
  readonly planMode = $derived(this.permissionModes ? isPlanMode(this.harnessMode?.mode) : this.prefs.planMode);

  readonly models = $derived(harnessModels(this.harness));
  readonly modelsError = $derived(harnessModelsError(this.harness));
  private readonly modelList = $derived<Model[]>(this.models ?? []);
  private readonly selectedModel = $derived(this.modelList.find((model) => model.id === this.prefs.model) ?? null);
  private readonly defaultModel = $derived(this.modelList.find((model) => model.isDefault) ?? null);
  readonly effortOptions = $derived((this.selectedModel ?? this.defaultModel)?.supportedReasoningEfforts ?? []);
  readonly subagentEfforts = $derived([
    ...new Set(
      this.modelList.flatMap((model) => model.supportedReasoningEfforts.map((option) => option.reasoningEffort)),
    ),
  ]);
  readonly modelButtonLabel = $derived.by(() => {
    const name = this.selectedModel?.displayName ?? "Model";
    return this.prefs.effort ? `${name} · ${this.prefs.effort}` : name;
  });

  private readonly effectiveModel = $derived(this.prefs.model ?? this.defaultModel?.id ?? null);
  /** The effort a turn runs at, including the model default the user never touched. */
  private readonly effectiveEffort = $derived(
    this.prefs.effort ?? (this.selectedModel ?? this.defaultModel)?.defaultReasoningEffort ?? null,
  );

  private readonly speedModel = $derived(this.selectedModel ?? this.defaultModel);
  readonly supportsSpeed = $derived(this.models?.some((model) => model.speedTiers !== undefined) ?? false);
  readonly fastSpeedTier = $derived(fastTier(this.speedModel));
  private readonly selectedSpeedTier = $derived.by(
    () => this.prefs.speedTier ?? (this.deps.threadId() ? this.deps.threadSpeedTier() : undefined),
  );
  readonly selectedFast = $derived(fastModeState(this.selectedSpeedTier, this.speedModel));
  readonly displayedFast = $derived.by(() =>
    fastModeState(this.deps.busy() ? this.deps.runningSpeedTier() : this.selectedSpeedTier, this.speedModel),
  );
  readonly speedPending = $derived.by(
    () => this.deps.busy() && this.prefs.speedTier !== null && this.selectedFast !== this.displayedFast,
  );

  /**
   * Why this composer refuses to send, or null when it will. A turn carries the
   * model, permission preset and subagent policy with it, so a setting that
   * cannot produce a valid turn has to be fixed before the message goes out
   * rather than silently resolving to something the user did not choose.
   */
  readonly invalidReason = $derived.by(() => {
    if (
      policyIsEmpty(
        this.deps.subagentModelPolicy(),
        this.modelList.map((model) => model.id),
      )
    ) {
      return "Pick at least one subagent model";
    }
    if (policyIsEmpty(this.deps.subagentReasoningEffortPolicy(), this.subagentEfforts)) {
      return "Pick at least one subagent effort level";
    }
    if (!this.prefs.model) return "Choose a model";
    if (this.permissionModes ? !isModeOf(this.harness, this.harnessMode?.mode) : !this.prefs.permissionPreset) {
      return "Choose a permission mode";
    }
    return null;
  });

  constructor(private readonly deps: ComposerSettingsDeps) {
    this.prefs = untrack(() => loadScopedPrefs(deps.projectKey(), deps.threadId()));

    // `scope` is the thread the prefs were loaded for, so a scope change
    // mid-mount (a draft thread being adopted) is detectable.
    let scope = untrack(() => deps.threadId());
    $effect(() => {
      const threadId = deps.threadId();
      if (threadId === scope) return;
      // A draft thread that just became a real one keeps the choices the user
      // made while drafting; any other scope change loads that scope's own prefs.
      if (scope === null && threadId)
        saveScopedPrefs(
          untrack(deps.projectKey),
          threadId,
          untrack(() => this.prefs),
        );
      else this.prefs = loadScopedPrefs(untrack(deps.projectKey), threadId);
      scope = threadId;
    });

    // Reconcile the thread's subagent policy with the remembered one. A thread
    // that carries its own policy wins — that is what Codex will actually
    // enforce — and a thread with none adopts the remembered choices, which is
    // how a new thread inherits the last-used ones.
    $effect(() => {
      void deps.threadId();
      const model = deps.subagentModelPolicy();
      const effort = deps.subagentReasoningEffortPolicy();
      untrack(() => {
        let seed = false;
        if (model !== null) this.prefs.subagentModelPolicy = model;
        else seed ||= this.prefs.subagentModelPolicy !== null;
        if (effort !== null) this.prefs.subagentReasoningEffortPolicy = effort;
        else seed ||= this.prefs.subagentReasoningEffortPolicy !== null;
        if (seed) deps.onSubagentPolicyChange(this.prefs.subagentModelPolicy, this.prefs.subagentReasoningEffortPolicy);
      });
    });

    // A speed override that matches what the thread already runs at is no
    // override.
    $effect(() => {
      if (this.prefs.speedTier !== null && this.prefs.speedTier === deps.threadSpeedTier()) {
        this.prefs.speedTier = null;
        this.persist();
      }
    });

    $effect(() => {
      deps.onModelChange(this.effectiveModel);
    });

    // Every turn sends an explicit collaboration mode (plan or default), and its
    // settings need a concrete model, so make sure the model list is available
    // before the first send.
    $effect(() => {
      void this.ensureModels();
    });
  }

  ensureModels(): Promise<void> {
    return ensureHarnessModels(this.harness);
  }

  /** What a turn sent right now would run with: the overrides plus the
   *  resolved model/effort pair the transcript labels replies with. */
  turnOptions(): TurnOptions | undefined {
    const options = turnOptionsFrom(
      this.prefs,
      this.deps.subagentModelPolicy(),
      this.deps.subagentReasoningEffortPolicy(),
      this.defaultModel?.id ?? null,
      this.deps.threadModel(),
      this.harness,
    );
    if (options && !this.supportsSpeed) delete options.speedTier;
    if (this.collaborationModes && !options?.collaborationMode) {
      console.warn("composer: no model resolved; turn sent without an explicit collaboration mode");
    }
    if (!this.effectiveModel && !this.effectiveEffort) return options;
    return { ...options, resolvedModel: this.effectiveModel, resolvedEffort: this.effectiveEffort };
  }

  /** Whether a send has to wait for the model list to back its settings. */
  get needsModels(): boolean {
    return !this.models && !this.prefs.model && !this.deps.threadModel();
  }

  persist(): void {
    saveScopedPrefs(this.deps.projectKey(), this.deps.threadId(), this.prefs);
  }

  toggleFastMode(): void {
    const next = this.selectedFast === true ? "default" : this.fastSpeedTier;
    if (!next) return;
    this.prefs.speedTier = next;
    this.persist();
    this.deps.onLiveSettingsChange({ speedTier: this.prefs.speedTier });
  }

  chooseModel(model: Model): void {
    const prefs = this.prefs;
    const keepFast = this.selectedFast === true;
    prefs.model = model.id;
    if (keepFast) prefs.speedTier = fastTier(model) ?? "default";
    if (!model.supportedReasoningEfforts.some((option) => option.reasoningEffort === prefs.effort)) {
      prefs.effort = model.defaultReasoningEffort;
    }
    this.persist();
    this.deps.onLiveSettingsChange({
      model: prefs.model ?? null,
      effort: prefs.effort ?? null,
      ...(keepFast && prefs.speedTier ? { speedTier: prefs.speedTier } : {}),
    });
  }

  chooseEffort(effort: string): void {
    this.prefs.effort = effort;
    this.persist();
    this.deps.onLiveSettingsChange({ model: this.prefs.model ?? null, effort: this.prefs.effort ?? null });
  }

  /** A preset (or, with permission modes, a mode). There is no un-choosing back
   *  to "unset": a turn needs one, and sending is blocked without it. */
  choosePermission(id: string): void {
    if (this.permissionModes) {
      if (!isModeOf(this.harness, id)) return;
      selectMode(this.prefs, id);
    } else this.prefs.permissionPreset = id;
    this.persist();
  }

  /** Follow a mode the harness switched to on its own. */
  syncPermissionMode(mode: string): boolean {
    if (!this.permissionModes || !isModeOf(this.harness, mode)) return false;
    selectMode(this.prefs, mode);
    this.persist();
    return true;
  }

  /** Enter plan mode, or leave it for the mode it was entered from. */
  setPlanMode(on: boolean): void {
    if (this.permissionModes) {
      const plan = this.modeOptions.find((mode) => mode.plan)?.id;
      const back = this.harnessMode?.exitPlanMode ?? this.modeOptions.find((mode) => !mode.plan)?.id;
      const next = on ? plan : back;
      if (next) selectMode(this.prefs, next);
    } else this.prefs.planMode = on;
    this.persist();
  }

  /** Codex ids and Claude aliases do not overlap, so a switch drops the
   *  picked model and effort and lets the other harness's defaults apply. */
  chooseHarness(next: HarnessChoice): void {
    if (next === this.harness) return;
    this.prefs.harness = next;
    this.prefs.model = null;
    this.prefs.effort = null;
    this.persist();
  }

  toggleSubagentModel(id: string): void {
    const all = this.modelList.map((model) => model.id);
    this.setSubagentPolicy(
      togglePolicy(this.deps.subagentModelPolicy(), all, id),
      this.deps.subagentReasoningEffortPolicy(),
    );
  }

  toggleSubagentEffort(effort: string): void {
    this.setSubagentPolicy(
      this.deps.subagentModelPolicy(),
      togglePolicy(this.deps.subagentReasoningEffortPolicy(), this.subagentEfforts, effort),
    );
  }

  /** Apply a subagent policy change to the thread and remember it for the next one. */
  private setSubagentPolicy(modelPolicy: SubagentPolicy | null, effortPolicy: SubagentPolicy | null): void {
    this.prefs.subagentModelPolicy = modelPolicy;
    this.prefs.subagentReasoningEffortPolicy = effortPolicy;
    this.persist();
    this.deps.onSubagentPolicyChange(modelPolicy, effortPolicy);
  }
}

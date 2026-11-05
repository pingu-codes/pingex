<script lang="ts">
  import { ArrowUp, Map as MapIcon, Paperclip, Square, Target } from "@lucide/svelte";
  import { openMemories } from "$lib/app/navigation.svelte";
  import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import AdaptiveToolbar from "$lib/composer/AdaptiveToolbar.svelte";
  import { AttachmentStager } from "$lib/composer/attachments.svelte";
  import ContextMeter from "$lib/composer/ContextMeter.svelte";
  import { ComposerDraft, HistoryRecall } from "$lib/composer/composerDraft.svelte";
  import {
    buildGoalObjective,
    buildTurnInput,
    type ComposerPart,
    cloneParts,
    type DetectedQueries,
    EMPTY_PARTS,
    hasSendableContent,
    normaliseParts,
    plainText,
    trimParts,
  } from "$lib/composer/composerParts";
  import type { HarnessChoice } from "$lib/composer/composerPrefs";
  import { ComposerSettings } from "$lib/composer/composerSettings.svelte";
  import HarnessMenu from "$lib/composer/HarnessMenu.svelte";
  import MentionPicker from "$lib/composer/MentionPicker.svelte";
  import ModelPopover from "$lib/composer/ModelPopover.svelte";
  import PermissionsPopover from "$lib/composer/PermissionsPopover.svelte";
  import ReviewTargetPicker from "$lib/composer/ReviewTargetPicker.svelte";
  import { RichEditor } from "$lib/composer/richEditor.svelte";
  import SkillPicker from "$lib/composer/SkillPicker.svelte";
  import SlashCommandPicker from "$lib/composer/SlashCommandPicker.svelte";
  import SubagentPolicyPopover from "$lib/composer/SubagentPolicyPopover.svelte";
  import { skillLabel } from "$lib/composer/skills";
  import { INIT_PROMPT, parseSlashCommand, type SlashCommand, type SlashCommandId } from "$lib/composer/slashCommands";
  import { threadHomeLabel } from "$lib/services/homeRouting";
  import { openSettings, settingsNav } from "$lib/services/settingsNav.svelte";
  import type { ContextStats } from "$lib/thread/contextUsage";
  import { freshPlanPrompt } from "$lib/thread/planHandoff";
  import type { ReviewTarget, SkillSummary, SubagentPolicy, TurnOptions, UserInputPart } from "$lib/types";
  import { can } from "$lib/utils/capabilities";
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
    threadSpeedTier = undefined,
    runningSpeedTier = undefined,
    history = [],
    threadHarness = null,
    onSend,
    onInterrupt,
    onCommand,
    onGoal,
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
    /** Resolving `false` means the message reached nothing — the harness
     *  refused it — and the composer gives the text back. */
    onSend: (input: UserInputPart[], options?: TurnOptions) => Promise<boolean> | boolean | undefined;
    onInterrupt: () => void;
    /** Thread-level slash commands (compact, new, fork, archive, rename).
     *  `argument` is whatever followed the command name, e.g. `/undo 2`. `typed`
     *  is the line as submitted, so a handler that fails can put it back via
     *  `restoreText` rather than leaving the user with an empty composer. */
    onCommand?: (command: SlashCommandId, argument?: string, typed?: string) => void;
    /** Goal mode's send: set `objective` as the thread's goal. Resolves false
     *  when it did not happen (declined or failed) so the composer can give the
     *  text back. Absent = no goal support, and no Goal toggle. */
    onGoal?: (objective: string, edit: boolean) => Promise<boolean>;
    /** A review target chosen in the picker `openReviewPicker()` opened. */
    onReview?: (target: ReviewTarget) => void;
    /** Run the plan in a fresh thread instead of this one; absent = no such option. */
    onImplementFresh?: (input: UserInputPart[], options?: TurnOptions) => void;
    onSubagentPolicyChange?: (modelPolicy: SubagentPolicy | null, effortPolicy: SubagentPolicy | null) => void;
    /** The model turns will actually run on — the picked one, else Codex's default. */
    onModelChange?: (modelId: string | null) => void;
    /** The user picked a model or effort; the owner decides whether a running
     *  turn can take it now (`turn/settings/update`) or from the next turn. */
    onLiveSettingsChange?: (settings: { model?: string | null; effort?: string | null; speedTier?: string }) => void;
    /** The model the thread last ran on; backs the collaboration-mode settings
     *  when nothing is picked and the model list has not loaded yet. */
    threadModel?: string | null;
    threadSpeedTier?: string | null;
    runningSpeedTier?: string | null;
    /** Prior user messages, oldest first, for ↑/↓ recall from the composer's edges. */
    history?: string[];
    /** The harness an existing thread runs on. A draft reads the pref instead. */
    threadHarness?: HarnessChoice | null;
  } = $props();

  let editor = $state<HTMLDivElement | null>(null);
  let composerBox = $state<HTMLDivElement | null>(null);

  const attachments = new AttachmentStager({
    editor: () => richEditor,
    dropZone: () => composerBox,
    disabled: () => disabled,
  });

  // The editor proper: parts, caret, undo and trigger queries live in the
  // controller; this component renders it, wires its DOM events and supplies
  // the decisions it does not own (submit, history, pickers, staging).
  const richEditor: RichEditor = new RichEditor({
    cwd: () => cwd,
    chipHandlers: attachments.chipHandlers,
    onChipRemoved: (chip) => attachments.chipRemoved(chip),
    pickerActive: () => pickerActive,
    pickerOpen: () => pickerOpen,
    suppressQueries: () => reviewPicker,
    onSubmit: () => submit(),
    onHistory: (direction) => recall.recall(direction),
    onEdit: () => recall.reset(),
  });

  const recall = new HistoryRecall(richEditor, () => history ?? []);

  $effect(() => {
    if (editor) return richEditor.attach(editor);
  });

  const draft = new ComposerDraft(richEditor, () => draftKey);

  const settings = new ComposerSettings({
    projectKey: () => projectKey,
    threadId: () => threadId,
    threadHarness: () => threadHarness,
    threadModel: () => threadModel,
    threadSpeedTier: () => threadSpeedTier,
    runningSpeedTier: () => runningSpeedTier,
    busy: () => busy,
    subagentModelPolicy: () => subagentModelPolicy,
    subagentReasoningEffortPolicy: () => subagentReasoningEffortPolicy,
    onSubagentPolicyChange: (modelPolicy, effortPolicy) => onSubagentPolicyChange?.(modelPolicy, effortPolicy),
    onModelChange: (modelId) => onModelChange?.(modelId),
    onLiveSettingsChange: (live) => onLiveSettingsChange?.(live),
  });
  const prefs = $derived(settings.prefs);
  const harness = $derived(settings.harness);
  const permissionModes = $derived(settings.permissionModes);

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

  let popover = $state<"model" | "subagents" | "permissions" | null>(null);

  // The @-mention, /-command and $-skill pickers are driven by the trigger
  // queries the editor detects (`richEditor.queries`).
  const queries: DetectedQueries = $derived(richEditor.queries);

  // `/review` target picker state. Unlike the three above it is not driven by a
  // trigger character, so it opens on request and filters on the whole line.
  let reviewPicker = $state(false);
  const reviewQuery = $derived(plainText(richEditor.parts) ?? "");

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

  const hasSendable = $derived(hasSendableContent(richEditor.parts));
  const invalidReason = $derived(settings.invalidReason);

  async function togglePopover(which: "model" | "subagents" | "permissions") {
    popover = popover === which ? null : which;
    if (popover === "model" || popover === "subagents") await settings.ensureModels();
  }

  function choosePermission(id: string) {
    settings.choosePermission(id);
    popover = null;
  }

  function togglePlanMode() {
    settings.setPlanMode(!settings.planMode);
  }

  // Goal mode: the next send sets the thread's goal from the editor instead of
  // starting a turn, then the mode switches itself off. It is not a preference —
  // it never outlives one send — so it is not persisted with the others.
  let goalMode = $state(false);
  // The banner's Edit put the current objective in the editor; whatever the user
  // had typed before is parked in `stashedParts` until the edit ends either way.
  let goalEditing = $state(false);
  let stashedParts: ComposerPart[] | null = null;
  const goalAvailable = $derived(Boolean(onGoal) && can(harness, "goals"));

  function toggleGoalMode() {
    if (goalEditing) {
      cancelGoalEdit();
      return;
    }
    goalMode = !goalMode;
  }

  /** Bring an existing goal into the composer for editing; the send confirms it. */
  export function startGoalEdit(objective: string) {
    if (!goalEditing) stashedParts = cloneParts(richEditor.parts);
    richEditor.setText(objective);
    goalMode = true;
    goalEditing = true;
    editor?.focus();
  }

  function endGoalEdit() {
    goalMode = false;
    goalEditing = false;
    richEditor.setParts(stashedParts ?? EMPTY_PARTS());
    stashedParts = null;
  }

  function cancelGoalEdit() {
    if (!goalEditing) return;
    endGoalEdit();
    editor?.focus();
  }

  // The plan the user already acted on ("Keep planning" or "Implement the
  // plan"), so the action bar only reappears when a newer plan arrives.
  let dismissedPlan = $state<string | null>(null);
  const showPlanActions = $derived(
    settings.planMode &&
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
    settings.setPlanMode(false);
  }

  /** The harness moved the thread between modes on its own: follow it, so the
   *  pill says what the next turn will actually run as. */
  export function syncPlanMode(on: boolean) {
    if (settings.planMode === on) return;
    if (on) settings.setPlanMode(true);
    else leavePlanMode();
  }

  export function syncPermissionMode(mode: string) {
    if (settings.syncPermissionMode(mode) && mode !== "plan") dismissedPlan = plan;
  }

  /**
   * Which harness a draft starts on. Read by ThreadView at thread creation;
   * an existing thread never switches.
   */
  export function harnessChoice(): HarnessChoice | null {
    return prefs.harness;
  }

  export function implementPlan() {
    leavePlanMode();
    onSend([{ type: "text", text: "Implement the plan." }], turnOptions());
  }

  /** Implement the plan in a new thread, carrying the plan across as its only
   *  context — the planning conversation itself is left behind. */
  export function implementPlanFresh(planText: string | null = plan) {
    if (!planText || !onImplementFresh) return;
    leavePlanMode();
    onImplementFresh([{ type: "text", text: freshPlanPrompt(planText) }], turnOptions());
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
    } else if (command.id === "goal" && !argument && goalAvailable) {
      toggleGoalMode();
    } else if (command.id === "model" || command.id === "permissions") {
      togglePopover(command.id);
    } else if (command.id === "init") {
      // `/init` is a prompt, not an action: put it in the composer so the user
      // can adjust the wording before sending.
      richEditor.setText(INIT_PROMPT);
    } else if (command.id === "memories") {
      openMemories();
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

  /** What a turn sent right now would run with — for sends that bypass the
   *  composer's text, such as an edited message. */
  export function turnOptions(): TurnOptions | undefined {
    return settings.turnOptions();
  }

  /** True when nothing sendable has been typed or attached. */
  export function isEmpty(): boolean {
    return !hasSendableContent(normaliseParts(cloneParts(richEditor.parts)));
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

  function submit() {
    // Enter belongs to the review picker while it is open, even when its list is
    // empty — its filter text is not a message.
    if (reviewPicker) return;
    // A submitted `/command` runs rather than being sent as a message. The picker
    // has already closed by this point if an argument was typed, so this — not
    // the picker — is what makes `/review the auth changes` work.
    const typed = plainText(richEditor.parts);
    const command = typed !== null ? parseSlashCommand(typed) : null;
    if (command) {
      runCommand(command.command, command.argument);
      return;
    }
    const trimmedParts = trimParts(richEditor.parts);
    if (!hasSendableContent(trimmedParts) || disabled || hasQuestions || invalidReason) return;
    if (goalMode && onGoal) {
      void submitGoal(trimmedParts);
      return;
    }
    const sent = buildTurnInput(trimmedParts, cwd);
    const snapshot = cloneParts(trimmedParts);
    richEditor.clear();
    attachments.forget();
    closePickers();
    draft.discard();
    void dispatchSend(sent, snapshot);
  }

  /**
   * Goal mode's send: the editor becomes the objective and no turn starts. The
   * editor clears optimistically and comes back — chips and all — if the goal
   * was not set. An edit keeps the objective in place on failure so the user can
   * retry or cancel, and restores the parked draft on success.
   */
  async function submitGoal(parts: ComposerPart[]) {
    if (!onGoal) return;
    const objective = buildGoalObjective(parts, cwd);
    const editing = goalEditing;
    const snapshot = cloneParts(parts);
    if (!editing) {
      richEditor.clear();
      goalMode = false;
    }
    closePickers();
    draft.discard();
    const ok = await onGoal(objective, editing).catch(() => false);
    if (editing) {
      if (ok) endGoalEdit();
      return;
    }
    if (ok) {
      attachments.forget();
    } else {
      goalMode = true;
      richEditor.setParts(snapshot);
    }
  }

  /** Send once a model can back the collaboration settings: the first send after
   *  launch may beat the model list, and a mode-less turn is what leaves a thread
   *  stuck in plan mode. Bounded so a failing model fetch never blocks sending. */
  async function dispatchSend(sent: UserInputPart[], snapshot: ComposerPart[]) {
    if (settings.needsModels) {
      await Promise.race([settings.ensureModels(), new Promise((resolve) => setTimeout(resolve, 3000))]);
    }
    let outcome: boolean | undefined;
    try {
      outcome = await onSend(sent, turnOptions());
    } catch {
      outcome = false;
    }
    // The message reached nothing: give the text back rather than losing it.
    if (outcome === false && isEmpty()) richEditor.setParts(snapshot);
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key !== "Escape" || event.defaultPrevented) return;
    // The Settings overlay owns Escape while open (it closes the panel); an
    // Escape there must not reach through and interrupt the running turn.
    if (settingsNav.open) return;
    if (popover !== null) {
      popover = null;
      return;
    }
    // The mention/slash/skill pickers own Escape while open.
    if (pickerOpen) return;
    if (goalEditing) {
      cancelGoalEdit();
      return;
    }
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
      <div class="mb-2 flex items-center gap-2 panel-card px-3 py-2">
        <span class="min-w-0 flex-1 truncate text-xs text-surface-500"
          >Plan ready — implement it, or keep refining in plan mode.</span
        >
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
        <button onclick={implementPlan} class="shrink-0 rounded-full px-2.5 py-1 text-[11px] preset-filled-primary-500">
          {permissionModes
            ? `Implement with ${settings.modeOptions.find((mode) => mode.id === settings.harnessMode?.exitPlanMode)?.label ?? "Default"}`
            : "Implement the plan"}
        </button>
      </div>
    {/if}
    {#if goalEditing}
      <div class="mb-2 flex items-center gap-2 panel-card px-3 py-2" data-testid="goal-edit-bar">
        <Target size={12} class="shrink-0 text-primary-500" />
        <span class="min-w-0 flex-1 truncate text-xs text-surface-500"
          >Editing the goal — send to update it, or cancel.</span
        >
        <button
          onclick={cancelGoalEdit}
          aria-label="Cancel goal edit"
          class="shrink-0 rounded-full px-2.5 py-1 text-[11px] text-surface-600-400 transition hover:bg-surface-200-800 hover:text-surface-800-200"
        >
          Cancel
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
        harness={settings.harness}
        scope={editor}
        onPick={(command) => runCommand(command)}
        onClose={() => richEditor.closeQueries()}
        onCount={(count) => (pickerCount = count)}
      />
    {/if}

    <div
      bind:this={composerBox}
      ondrop={attachments.handleDrop}
      ondragover={attachments.handleDragOver}
      ondragenter={attachments.handleDragOver}
      ondragleave={attachments.handleDragLeave}
      role="group"
      aria-label="Message composer"
      class="rounded-2xl border bg-surface-100-900 px-3 py-2 transition-colors {attachments.dragOver
        ? "border-primary-500 bg-primary-500/5"
        : "border-surface-200-800 focus-within:border-surface-400-600"}"
    >
      {#if attachments.dragOver}
        <div class="pointer-events-none mb-1 text-center text-[11px] font-medium text-primary-600-400">
          Drop files to attach
        </div>
      {/if}
      <input
        bind:this={attachments.fileInput}
        type="file"
        multiple
        class="hidden"
        aria-hidden="true"
        tabindex={-1}
        onchange={attachments.handleFileInputChange}
      >
      <div class="flex min-w-0 items-end gap-2">
        <div
          bind:this={editor}
          contenteditable={!disabled}
          spellcheck="false"
          {...{ autocorrect: "off", autocapitalize: "off" } as Record<string, string>}
          tabindex={disabled ? -1 : 0}
          role="textbox"
          aria-multiline="true"
          aria-label="Message Codex… (@ to attach files, / for commands)"
          data-placeholder="Message Codex… (@ to attach files, / for commands)"
          onkeydown={richEditor.handleKey}
          oninput={richEditor.handleInput}
          onclick={richEditor.handleClick}
          onpaste={attachments.handlePaste}
          oncompositionstart={richEditor.handleCompositionStart}
          oncompositionend={richEditor.handleCompositionEnd}
          class="composer-editor min-w-0 flex-1 overflow-y-auto bg-transparent text-sm leading-6 outline-none empty:before:pointer-events-none empty:before:text-surface-500 empty:before:content-[attr(data-placeholder)] {disabled
            ? "pointer-events-none opacity-50"
            : ""}"
        ></div>
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
          label={invalidReason ??
            (hasQuestions
              ? "Answer questions"
              : busy && hasSendable
                ? "Queue message — sends when Codex finishes"
                : "Send message")}
          class="shrink-0"
        >
          <button
            onclick={submit}
            aria-label={goalEditing ? "Update goal" : goalMode ? "Set goal" : busy ? "Queue message" : "Send message"}
            disabled={disabled || !hasSendable || hasQuestions || Boolean(invalidReason)}
            class="grid size-7 place-items-center rounded-full preset-filled-primary-500 disabled:opacity-40"
          >
            <ArrowUp size={14} />
          </button>
        </TooltipAnchor>
      </div>

      {#snippet attachment()}
        <TooltipButton
          label="Attach files"
          onclick={() => attachments.pickFiles()}
          {disabled}
          aria-label="Attach files"
          class="inline-flex items-center rounded-full px-1.5 py-1 text-surface-500 transition hover:bg-surface-200-800 hover:text-surface-800-200 disabled:opacity-40"
        >
          <Paperclip size={14} />
        </TooltipButton>
      {/snippet}
      {#snippet plan()}
        {#if settings.collaborationModes}
          <TooltipButton
            label={prefs.planMode ? "Plan mode on" : "Toggle plan mode"}
            onclick={togglePlanMode}
            aria-label="Toggle plan mode"
            aria-pressed={prefs.planMode}
            class={prefs.planMode ? "pill-active" : "pill"}
          >
            <MapIcon size={12} />
            Plan
          </TooltipButton>
        {/if}
      {/snippet}
      {#snippet goal()}
        {#if goalAvailable}
          <TooltipButton
            label={goalEditing
              ? "Cancel goal edit"
              : goalMode
                ? "Goal mode on — send sets the goal"
                : "Toggle goal mode"}
            onclick={toggleGoalMode}
            aria-label="Toggle goal mode"
            aria-pressed={goalMode}
            class={goalMode ? "pill-active" : "pill"}
          >
            <Target size={12} />
            Goal
          </TooltipButton>
        {/if}
      {/snippet}
      {#snippet model()}
        <ModelPopover
          open={popover === "model"}
          models={settings.models}
          modelsError={settings.modelsError}
          modelId={prefs.model}
          effort={prefs.effort}
          effortOptions={settings.effortOptions}
          label={settings.modelButtonLabel}
          onToggle={() => togglePopover("model")}
          onChooseModel={(model) => settings.chooseModel(model)}
          onChooseEffort={(effort) => settings.chooseEffort(effort)}
        />
      {/snippet}
      {#snippet fast()}
        {#if settings.supportsSpeed}
          {@const { fastSpeedTier, selectedFast, displayedFast, speedPending } = settings}
          <TooltipButton
            label={!fastSpeedTier && selectedFast !== true
              ? "Fast mode is unavailable for this model"
              : speedPending
                ? "The selected speed applies to the next turn until the running turn confirms it"
                : "Toggle fast mode for this thread"}
            aria-label="Fast mode"
            aria-pressed={displayedFast === undefined ? "mixed" : displayedFast}
            disabled={disabled || (!fastSpeedTier && selectedFast !== true)}
            onclick={() => settings.toggleFastMode()}
            class="{displayedFast ? "pill-active" : "pill"} gap-1"
          >
            Fast {displayedFast === undefined ? "unknown" : displayedFast ? "on" : "off"}
            {#if speedPending}
              <span>· Next {selectedFast ? "on" : "off"}</span>
            {/if}
          </TooltipButton>
        {/if}
      {/snippet}
      {#snippet subagents()}
        <SubagentPolicyPopover
          open={popover === "subagents"}
          models={settings.models}
          modelsError={settings.modelsError}
          efforts={settings.subagentEfforts}
          modelPolicy={subagentModelPolicy}
          effortPolicy={subagentReasoningEffortPolicy}
          onToggle={() => togglePopover("subagents")}
          onToggleModel={(id) => settings.toggleSubagentModel(id)}
          onToggleEffort={(effort) => settings.toggleSubagentEffort(effort)}
        />
      {/snippet}
      {#snippet permissions()}
        <PermissionsPopover
          open={popover === "permissions"}
          selectedId={permissionModes ? (settings.harnessMode?.mode ?? null) : prefs.permissionPreset}
          onToggle={() => togglePopover("permissions")}
          {harness}
          onChoose={choosePermission}
        />
      {/snippet}
      {#snippet home()}
        {#if !threadId}
          <HarnessMenu {harness} onChoose={(next) => settings.chooseHarness(next)} />
        {:else if threadHomeLabel(threadId)}
          <span class="max-w-56 truncate text-[10px] text-surface-500" title={threadHomeLabel(threadId)}
            >{threadHomeLabel(threadId)}</span
          >
        {/if}
      {/snippet}
      {#snippet context()}
        <ContextMeter
          stats={contextStats}
          {compacting}
          busy={busy || disabled}
          onCompact={onCommand && can(settings.harness, "compact") ? () => onCommand("compact") : undefined}
        />
      {/snippet}
      <AdaptiveToolbar
        {attachment}
        {model}
        {context}
        activeItem={popover}
        onLayoutChange={() => (popover = null)}
        items={[
          { id: "plan", priority: 5, placement: "beforeModel", content: plan },
          ...(goalAvailable ? [{ id: "goal", priority: 4, placement: "beforeModel" as const, content: goal }] : []),
          ...(settings.supportsSpeed ? [{ id: "fast", priority: 3, content: fast }] : []),
          { id: "subagents", priority: 1, content: subagents },
          { id: "permissions", priority: 2, content: permissions },
          ...(!threadId || threadHomeLabel(threadId)
            ? [{ id: "home", priority: 0, placement: "end" as const, content: home }]
            : []),
        ]}
      />
    </div>
  </div>
</div>

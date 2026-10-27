import { HARNESS_MODES } from "$lib/bindings";
import type { ComposerPrefs } from "$lib/composer/composerPrefs";
import type { HarnessKind, HarnessMode } from "$lib/types";

/** One entry in a harness's permissions menu (`HarnessKind::modes`). */
export interface ModeOption {
  id: string;
  label: string;
  description: string;
  /** The mode plans instead of acting. */
  plan: boolean;
}

const MODES = HARNESS_MODES as Partial<Record<HarnessKind, readonly ModeOption[]>>;

/** The modes a harness switches between itself, default first. Empty for a
 *  harness that takes presets instead (Codex). */
export function modesFor(harness: string | null | undefined): readonly ModeOption[] {
  return MODES[(harness ?? "codex") as HarnessKind] ?? [];
}

export function isModeOf(harness: string | null | undefined, mode: unknown): mode is string {
  return modesFor(harness).some((option) => option.id === mode);
}

/** Whether any harness has `mode` (for events that do not say which). */
export function isKnownMode(mode: unknown): mode is string {
  return Object.values(MODES).some((modes) => modes?.some((option) => option.id === mode));
}

/** Whether `mode` plans instead of acting, on whichever harness has it. */
export function isPlanMode(mode: string | null | undefined): boolean {
  return Object.values(MODES).some((modes) => modes?.some((option) => option.id === mode && option.plan));
}

/** The mode a harness leaves plan mode for when nothing else was chosen. */
function defaultMode(harness: string | null | undefined): string | null {
  return modesFor(harness).find((option) => !option.plan)?.id ?? null;
}

/** Old shared preset IDs remain readable without changing Codex preferences.
 *  They map onto Claude's modes; a harness without those ignores them. */
function fromPresets(prefs: ComposerPrefs): HarnessMode | null {
  const execution = {
    "read-only": "default",
    auto: "acceptEdits",
    "auto-review": "auto",
    "full-access": "bypassPermissions",
  } as const;
  const exitPlanMode = execution[prefs.permissionPreset as keyof typeof execution];
  if (!exitPlanMode && !prefs.planMode) return null;
  return { mode: prefs.planMode ? "plan" : exitPlanMode, exitPlanMode: exitPlanMode ?? "default" };
}

/**
 * The harness's saved mode, when it is one of its own. One slot serves every
 * harness, so plan mode carries across a harness switch and a mode the new
 * harness lacks reads as unset.
 */
export function modeFromPrefs(prefs: ComposerPrefs, harness: string | null | undefined): HarnessMode | null {
  const saved = prefs.claudeMode ?? fromPresets(prefs);
  if (!saved || !isModeOf(harness, saved.mode)) return null;
  const exit = saved.exitPlanMode;
  const exitPlanMode = exit && isModeOf(harness, exit) && !isPlanMode(exit) ? exit : defaultMode(harness);
  return { mode: saved.mode, exitPlanMode };
}

/** Choose `mode`. Entering plan mode remembers the mode it was entered from. */
export function selectMode(prefs: ComposerPrefs, mode: string): void {
  const previous = prefs.claudeMode ?? fromPresets(prefs);
  prefs.claudeMode = {
    mode,
    exitPlanMode: isPlanMode(mode) ? (previous?.exitPlanMode ?? null) : mode,
  };
}

import type { ClaudeModeSettings, ClaudePermissionMode } from "$lib/bindings";
import type { ComposerPrefs } from "$lib/composer/composerPrefs.svelte";

export const CLAUDE_MODES: { id: ClaudePermissionMode; label: string; description: string }[] = [
  { id: "default", label: "Default", description: "Ask for approval when an action needs permission" },
  {
    id: "acceptEdits",
    label: "Accept edits",
    description: "Allow file edits; other actions can still require approval",
  },
  { id: "auto", label: "Auto", description: "Claude’s classifier reviews actions" },
  { id: "plan", label: "Plan", description: "Explore and plan before implementing" },
  { id: "bypassPermissions", label: "Bypass permissions", description: "Skip permission prompts" },
];

export function isClaudeMode(mode: unknown): mode is ClaudePermissionMode {
  return CLAUDE_MODES.some((option) => option.id === mode);
}

/** Old shared preset IDs remain readable without changing Codex preferences. */
export function claudeModeFromPrefs(prefs: ComposerPrefs): ClaudeModeSettings | null {
  if (prefs.claudeMode) return prefs.claudeMode;
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

export function selectClaudeMode(prefs: ComposerPrefs, mode: ClaudePermissionMode): void {
  const previous = claudeModeFromPrefs(prefs);
  prefs.claudeMode = {
    mode,
    exitPlanMode: mode === "plan" ? (previous?.exitPlanMode ?? "default") : mode,
  };
}

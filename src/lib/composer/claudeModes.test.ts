import { beforeEach, describe, expect, it } from "vitest";
import { claudeModeFromPrefs, selectClaudeMode } from "$lib/composer/claudeModes";
import {
  loadPrefs,
  loadScopedPrefs,
  saveScopedPrefs,
  saveThreadClaudeMode,
  turnOptionsFrom,
} from "$lib/composer/composerPrefs";

beforeEach(() => localStorage.clear());

describe("Claude modes across planning and restart", () => {
  it("keeps inherited settings when a background Plan event arrives before the composer mounts", () => {
    saveScopedPrefs("/repo", null, {
      ...loadPrefs(),
      model: "sonnet",
      effort: "high",
      permissionPreset: "full-access",
    });
    saveThreadClaudeMode("background", "plan");
    const prefs = loadScopedPrefs("/repo", "background");
    expect(prefs.model).toBe("sonnet");
    expect(prefs.effort).toBe("high");
    expect(prefs.claudeMode).toEqual({ mode: "plan", exitPlanMode: "bypassPermissions" });
    saveThreadClaudeMode("background", "auto");
    saveThreadClaudeMode("background", "plan");
    expect(loadScopedPrefs("/repo", "background").claudeMode).toEqual({ mode: "plan", exitPlanMode: "auto" });
  });
  it.each(["bypassPermissions", "auto", "acceptEdits", "default"] as const)(
    "restores %s after reopening a plan",
    (mode) => {
      const prefs = loadPrefs();
      selectClaudeMode(prefs, mode);
      selectClaudeMode(prefs, "plan");
      saveScopedPrefs("/repo", "thread", prefs);
      const restored = loadScopedPrefs("/repo", "thread");
      expect(turnOptionsFrom(restored, null, null, "sonnet", null, "claude")).toEqual({
        claudeMode: { mode: "plan", exitPlanMode: mode },
      });
      saveThreadClaudeMode("thread", mode);
      const implemented = loadScopedPrefs("/repo", "thread");
      expect(turnOptionsFrom(implemented, null, null, "sonnet", null, "claude")).toEqual({
        claudeMode: { mode, exitPlanMode: mode },
      });
      selectClaudeMode(implemented, "plan");
      expect(claudeModeFromPrefs(implemented)?.exitPlanMode).toBe(mode);
    },
  );

  it.each([
    ["read-only", "default"],
    ["auto", "acceptEdits"],
    ["auto-review", "auto"],
    ["full-access", "bypassPermissions"],
  ] as const)("preserves the legacy %s selection under Plan", (preset, mode) => {
    const prefs = { ...loadPrefs(), permissionPreset: preset, planMode: true };
    saveScopedPrefs("/repo", "legacy", prefs);
    expect(claudeModeFromPrefs(loadScopedPrefs("/repo", "legacy"))).toEqual({ mode: "plan", exitPlanMode: mode });
  });

  it("keeps the last execution mode when a CLI confirms Plan and preserves other conversations", () => {
    const prefs = loadPrefs();
    selectClaudeMode(prefs, "bypassPermissions");
    saveScopedPrefs("/repo", "one", prefs);
    saveScopedPrefs("/repo", "two", prefs);
    saveThreadClaudeMode("one", "plan");
    saveThreadClaudeMode("one", "auto");
    expect(claudeModeFromPrefs(loadScopedPrefs("/repo", "one"))).toEqual({ mode: "auto", exitPlanMode: "auto" });
    expect(claudeModeFromPrefs(loadScopedPrefs("/repo", "two"))?.mode).toBe("bypassPermissions");
  });
});

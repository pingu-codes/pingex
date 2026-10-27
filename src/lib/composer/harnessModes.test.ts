import { beforeEach, describe, expect, it } from "vitest";
import {
  loadPrefs,
  loadScopedPrefs,
  saveScopedPrefs,
  saveThreadMode,
  turnOptionsFrom,
} from "$lib/composer/composerPrefs";
import { modeFromPrefs, modesFor, selectMode } from "$lib/composer/harnessModes";

beforeEach(() => localStorage.clear());

describe("Claude modes across planning and restart", () => {
  it("keeps inherited settings when a background Plan event arrives before the composer mounts", () => {
    saveScopedPrefs("/repo", null, {
      ...loadPrefs(),
      model: "sonnet",
      effort: "high",
      permissionPreset: "full-access",
    });
    saveThreadMode("background", "plan");
    const prefs = loadScopedPrefs("/repo", "background");
    expect(prefs.model).toBe("sonnet");
    expect(prefs.effort).toBe("high");
    expect(prefs.claudeMode).toEqual({ mode: "plan", exitPlanMode: "bypassPermissions" });
    saveThreadMode("background", "auto");
    saveThreadMode("background", "plan");
    expect(loadScopedPrefs("/repo", "background").claudeMode).toEqual({ mode: "plan", exitPlanMode: "auto" });
  });
  it.each(["bypassPermissions", "auto", "acceptEdits", "default"] as const)(
    "restores %s after reopening a plan",
    (mode) => {
      const prefs = loadPrefs();
      selectMode(prefs, mode);
      selectMode(prefs, "plan");
      saveScopedPrefs("/repo", "thread", prefs);
      const restored = loadScopedPrefs("/repo", "thread");
      expect(turnOptionsFrom(restored, null, null, "sonnet", null, "claude")).toEqual({
        mode: { mode: "plan", exitPlanMode: mode },
      });
      saveThreadMode("thread", mode);
      const implemented = loadScopedPrefs("/repo", "thread");
      expect(turnOptionsFrom(implemented, null, null, "sonnet", null, "claude")).toEqual({
        mode: { mode, exitPlanMode: mode },
      });
      selectMode(implemented, "plan");
      expect(modeFromPrefs(implemented, "claude")?.exitPlanMode).toBe(mode);
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
    expect(modeFromPrefs(loadScopedPrefs("/repo", "legacy"), "claude")).toEqual({ mode: "plan", exitPlanMode: mode });
  });

  it("keeps the last execution mode when a CLI confirms Plan and preserves other conversations", () => {
    const prefs = loadPrefs();
    selectMode(prefs, "bypassPermissions");
    saveScopedPrefs("/repo", "one", prefs);
    saveScopedPrefs("/repo", "two", prefs);
    saveThreadMode("one", "plan");
    saveThreadMode("one", "auto");
    expect(modeFromPrefs(loadScopedPrefs("/repo", "one"), "claude")).toEqual({ mode: "auto", exitPlanMode: "auto" });
    expect(modeFromPrefs(loadScopedPrefs("/repo", "two"), "claude")?.mode).toBe("bypassPermissions");
  });
});

describe("modes on another harness", () => {
  it("lists maki's own modes, default first", () => {
    expect(modesFor("maki").map((mode) => mode.id)).toEqual(["build", "yolo", "plan"]);
    expect(modesFor("codex")).toEqual([]);
  });

  it("carries plan mode across a harness switch and drops what the other lacks", () => {
    const prefs = loadPrefs();
    selectMode(prefs, "bypassPermissions");
    expect(modeFromPrefs(prefs, "maki")).toBeNull();
    selectMode(prefs, "plan");
    expect(modeFromPrefs(prefs, "maki")).toEqual({ mode: "plan", exitPlanMode: "build" });
    expect(modeFromPrefs(prefs, "claude")).toEqual({ mode: "plan", exitPlanMode: "bypassPermissions" });
    selectMode(prefs, "yolo");
    expect(turnOptionsFrom(prefs, null, null, null, null, "maki")).toEqual({
      mode: { mode: "yolo", exitPlanMode: "yolo" },
    });
    expect(turnOptionsFrom(prefs, null, null, null, null, "claude")).toBeUndefined();
  });
});

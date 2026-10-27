import { beforeEach, describe, expect, it } from "vitest";
import {
  type ComposerPrefs,
  loadPrefs,
  loadScopedPrefs,
  policyIsEmpty,
  presetFor,
  savePrefs,
  saveScopedPrefs,
  togglePolicy,
  turnOptionsFrom,
} from "$lib/composer/composerPrefs";

beforeEach(() => localStorage.clear());

describe("turnOptionsFrom", () => {
  it("sends an explicit standard tier when fast mode is disabled", () => {
    expect(turnOptionsFrom({ ...loadPrefs(), speedTier: "default" })).toEqual({ speedTier: "default" });
    expect(turnOptionsFrom({ ...loadPrefs(), speedTier: "priority" })).toEqual({ speedTier: "priority" });
  });
  it("returns undefined when nothing is chosen", () => {
    expect(turnOptionsFrom(loadPrefs())).toBeUndefined();
  });

  it("emits collaborationMode plan when plan mode is on", () => {
    const prefs = { ...loadPrefs(), planMode: true };
    expect(turnOptionsFrom(prefs, undefined, undefined, "gpt-5.2-codex")).toEqual({
      collaborationMode: {
        mode: "plan",
        settings: { model: "gpt-5.2-codex", reasoning_effort: null, developer_instructions: null },
      },
    });
  });

  it("falls back to the thread's last model when nothing else resolves", () => {
    expect(turnOptionsFrom(loadPrefs(), undefined, undefined, null, "gpt-5.6-sol")).toEqual({
      collaborationMode: {
        mode: "default",
        settings: { model: "gpt-5.6-sol", reasoning_effort: null, developer_instructions: null },
      },
    });
  });

  it("emits collaborationMode default when plan mode is off, so a thread left in plan mode exits it", () => {
    const prefs = { ...loadPrefs(), model: "gpt-5.2" };
    expect(turnOptionsFrom(prefs)).toEqual({
      model: "gpt-5.2",
      collaborationMode: {
        mode: "default",
        settings: { model: "gpt-5.2", reasoning_effort: null, developer_instructions: null },
      },
    });
  });

  it("omits collaborationMode when no model is resolvable", () => {
    expect(turnOptionsFrom({ ...loadPrefs(), effort: "high" })).toEqual({ effort: "high" });
  });

  it("migrates Claude Auto to its native mode and keeps Codex presets", () => {
    const prefs = { ...loadPrefs(), permissionPreset: "auto-review" };
    expect(turnOptionsFrom(prefs, undefined, undefined, null, null, "claude")).toEqual({
      mode: { mode: "auto", exitPlanMode: "auto" },
    });
    expect(turnOptionsFrom(prefs, undefined, undefined, null, null, "codex")).toEqual({
      approvalPolicy: "on-request",
      sandboxMode: "workspace-write",
    });
    expect(presetFor("auto-review")?.id).toBe("auto");
  });

  it("serializes subagent policies independently from the parent model", () => {
    expect(turnOptionsFrom(loadPrefs(), { allowed: ["gpt-5.6-terra"] }, { allowed: [] })).toEqual({
      subagentModelPolicy: { allowed: ["gpt-5.6-terra"] },
      subagentReasoningEffortPolicy: { allowed: [] },
    });
  });
});

describe("prefs persistence", () => {
  it("keeps speed choices per thread without seeding another thread or project", () => {
    saveScopedPrefs("/repo", "one", { ...loadPrefs(), speedTier: "priority" });
    expect(loadScopedPrefs("/repo", "one").speedTier).toBe("priority");
    expect(loadScopedPrefs("/repo", "two").speedTier).toBeNull();
    expect(loadScopedPrefs("/repo", null).speedTier).toBeNull();
    expect(loadScopedPrefs("/other", null).speedTier).toBeNull();
    saveScopedPrefs("/repo", "one", { ...loadScopedPrefs("/repo", "one"), speedTier: "default" });
    expect(loadScopedPrefs("/repo", "one").speedTier).toBe("default");
  });
  it("round-trips planMode through localStorage", () => {
    savePrefs({ ...loadPrefs(), planMode: true });
    expect(loadPrefs().planMode).toBe(true);
  });

  it("defaults planMode to false for stored prefs from before the field existed", () => {
    localStorage.setItem("pingu-composer-prefs", JSON.stringify({ model: "gpt-5.2" }));
    expect(loadPrefs().planMode).toBe(false);
    expect(localStorage.getItem("pingex-composer-prefs")).not.toBeNull();
  });
});

describe("scoped prefs", () => {
  const prefs = (patch: Partial<ComposerPrefs> = {}): ComposerPrefs => ({ ...loadPrefs(), ...patch });

  it("falls back to defaults for a project nobody has used", () => {
    expect(loadScopedPrefs("/repo", null)).toEqual({
      speedTier: null,
      harness: null,
      model: null,
      effort: null,
      permissionPreset: null,
      planMode: false,
      claudeMode: null,
      subagentModelPolicy: null,
      subagentReasoningEffortPolicy: null,
    });
  });

  it("seeds a new thread from the project's last-used choices", () => {
    saveScopedPrefs("/repo", "thread-a", prefs({ model: "gpt-5.2", planMode: true }));
    expect(loadScopedPrefs("/repo", null).model).toBe("gpt-5.2");
    expect(loadScopedPrefs("/repo", "thread-new").planMode).toBe(true);
  });

  it("keeps each thread's own choices once it has them", () => {
    saveScopedPrefs("/repo", "thread-a", prefs({ model: "gpt-5.2", planMode: true }));
    saveScopedPrefs("/repo", "thread-b", prefs({ model: "gpt-5.6-terra", planMode: false }));
    expect(loadScopedPrefs("/repo", "thread-a")).toMatchObject({ model: "gpt-5.2", planMode: true });
    expect(loadScopedPrefs("/repo", "thread-b")).toMatchObject({ model: "gpt-5.6-terra", planMode: false });
  });

  it("scopes last-used to the project", () => {
    saveScopedPrefs("/repo", null, prefs({ model: "gpt-5.2" }));
    expect(loadScopedPrefs("/other", null).model).toBeNull();
  });

  it("migrates pre-scoping global prefs into the fallback", () => {
    localStorage.setItem("pingex-composer-prefs", JSON.stringify({ model: "gpt-5.2", planMode: true }));
    expect(loadScopedPrefs("/repo", "thread-a")).toMatchObject({ model: "gpt-5.2", planMode: true });
    saveScopedPrefs("/repo", "thread-a", prefs({ model: "gpt-5.6-terra" }));
    expect(loadScopedPrefs("/other", null).model).toBe("gpt-5.2");
  });

  it("trims the oldest threads past the cap", () => {
    for (let index = 0; index < 320; index++) {
      saveScopedPrefs("/repo", `thread-${index}`, prefs({ effort: `e${index}` }));
    }
    const stored = JSON.parse(localStorage.getItem("pingex-composer-prefs") ?? "{}");
    expect(Object.keys(stored.threads)).toHaveLength(300);
    expect(stored.threads["thread-0"]).toBeUndefined();
    expect(stored.threads["thread-319"].effort).toBe("e319");
  });

  it("back-fills subagent fields onto entries written before they existed", () => {
    localStorage.setItem(
      "pingex-composer-prefs",
      JSON.stringify({ version: 2, fallback: null, projects: {}, threads: { "thread-a": { model: "gpt-5.2" } } }),
    );
    expect(loadScopedPrefs("/repo", "thread-a")).toMatchObject({
      subagentModelPolicy: null,
      subagentReasoningEffortPolicy: null,
    });
  });
});

describe("subagent prefs", () => {
  const prefs = (patch: Partial<ComposerPrefs> = {}): ComposerPrefs => ({ ...loadPrefs(), ...patch });
  const configured = () => prefs({ subagentModelPolicy: { allowed: ["gpt-5.2"] } });

  it("carries the last-used choices to a project nobody has configured", () => {
    saveScopedPrefs("/repo", "thread-a", configured());
    expect(loadScopedPrefs("/other", null)).toMatchObject({
      subagentModelPolicy: { allowed: ["gpt-5.2"] },
      // The rest of the prefs stay scoped to their own project.
      model: null,
    });
  });

  it("lets a project keep subagent choices that differ from the global ones", () => {
    saveScopedPrefs("/repo", null, configured());
    saveScopedPrefs("/other", null, prefs({ subagentModelPolicy: { excluded: ["gpt-5.2"] } }));
    expect(loadScopedPrefs("/repo", null).subagentModelPolicy).toEqual({ allowed: ["gpt-5.2"] });
    expect(loadScopedPrefs("/other", null).subagentModelPolicy).toEqual({ excluded: ["gpt-5.2"] });
  });

  it("round-trips the policies through localStorage", () => {
    saveScopedPrefs("/repo", "thread-a", prefs({ subagentReasoningEffortPolicy: { allowed: ["high"] } }));
    expect(loadScopedPrefs("/repo", "thread-a").subagentReasoningEffortPolicy).toEqual({ allowed: ["high"] });
  });
});

describe("policyIsEmpty", () => {
  it("treats no policy as allowing everything", () => {
    expect(policyIsEmpty(null, ["a", "b"])).toBe(false);
  });

  it("catches an allow-list with nothing on it", () => {
    expect(policyIsEmpty({ allowed: [] }, ["a", "b"])).toBe(true);
    expect(policyIsEmpty({ allowed: ["a"] }, ["a", "b"])).toBe(false);
  });

  it("catches an exclude-list that covers every known value", () => {
    expect(policyIsEmpty({ excluded: ["a", "b"] }, ["a", "b"])).toBe(true);
    expect(policyIsEmpty({ excluded: ["a"] }, ["a", "b"])).toBe(false);
  });
});

describe("togglePolicy", () => {
  it("narrows no policy to an allow-list without the value", () => {
    expect(togglePolicy(null, ["a", "b", "c"], "b")).toEqual({ allowed: ["a", "c"] });
  });

  it("reads an exclude-list as everything else allowed", () => {
    expect(togglePolicy({ excluded: ["a"] }, ["a", "b", "c"], "c")).toEqual({ allowed: ["b"] });
  });

  it("goes back to no policy once everything is allowed again", () => {
    expect(togglePolicy({ allowed: ["a", "c"] }, ["a", "b", "c"], "b")).toBeNull();
  });
});

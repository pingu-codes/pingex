import { describe, expect, it } from "vitest";
import { filterSkills, skillHint, skillLabel } from "$lib/composer/skills";
import type { SkillSummary } from "$lib/types";

function skill(overrides: Partial<SkillSummary> = {}): SkillSummary {
  return {
    name: "code-reviewer",
    path: "~/.codex/skills/code-reviewer/SKILL.md",
    scope: "user",
    description: "Review a diff for correctness and style.",
    enabled: true,
    displayName: null,
    shortDescription: null,
    ...overrides,
  };
}

describe("skillLabel", () => {
  it("prefers a plugin's display name over the namespaced protocol name", () => {
    expect(skillLabel(skill({ name: "browser-use:browser", displayName: "Browser" }))).toBe("Browser");
  });

  it("falls back to the name when there is no display name", () => {
    expect(skillLabel(skill())).toBe("code-reviewer");
    expect(skillLabel(skill({ displayName: "   " }))).toBe("code-reviewer");
  });
});

describe("skillHint", () => {
  it("prefers the short description, then the description, then the path", () => {
    expect(skillHint(skill({ shortDescription: "Short." }))).toBe("Short.");
    expect(skillHint(skill())).toBe("Review a diff for correctness and style.");
    expect(skillHint(skill({ description: null }))).toBe("~/.codex/skills/code-reviewer/SKILL.md");
  });
});

describe("filterSkills", () => {
  const all = [
    skill({ name: "agents-sdk", description: "Build AI agents on Cloudflare Workers." }),
    skill({ name: "browser-use:browser", displayName: "Browser", description: "Automate a browser." }),
    skill({ name: "code-reviewer", description: "Review a diff for correctness and style." }),
  ];

  it("returns everything for an empty query", () => {
    expect(filterSkills(all, "")).toEqual(all);
    expect(filterSkills(all, "   ")).toEqual(all);
  });

  it("matches a namespaced name by prefix", () => {
    expect(filterSkills(all, "browser-use:").map((s) => s.name)).toEqual(["browser-use:browser"]);
  });

  it("finds a skill by its description when the name does not match", () => {
    // The description is what the model matches on, so it is also the most
    // useful thing for a person to search by.
    expect(filterSkills(all, "diff").map((s) => s.name)).toEqual(["code-reviewer"]);
  });

  it("ranks name-prefix matches above description matches", () => {
    // "browser-use:browser" matches by prefix; nothing else does, but its own
    // description also contains "browser" — it must not be listed twice.
    const names = filterSkills(all, "browser").map((s) => s.name);
    expect(names).toEqual(["browser-use:browser"]);
  });

  it("matches a plugin skill by its display name", () => {
    expect(filterSkills(all, "Browser").map((s) => s.name)).toEqual(["browser-use:browser"]);
  });

  it("returns nothing when nothing matches", () => {
    expect(filterSkills(all, "zzz")).toEqual([]);
  });
});

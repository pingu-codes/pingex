import { describe, expect, it } from "vitest";
import { filterSections, SETTINGS_SECTIONS } from "$lib/layout/settingsSections";

describe("filterSections", () => {
  it("returns every section for an empty or whitespace query", () => {
    expect(filterSections(SETTINGS_SECTIONS, "")).toHaveLength(SETTINGS_SECTIONS.length);
    expect(filterSections(SETTINGS_SECTIONS, "   ")).toHaveLength(SETTINGS_SECTIONS.length);
  });

  it("matches on section label case-insensitively", () => {
    const result = filterSections(SETTINGS_SECTIONS, "APPEAR");
    expect(result.map((section) => section.id)).toEqual(["appearance"]);
  });

  it("matches on control keywords, not just the label", () => {
    // "sandbox" only appears as an Agent keyword, never in a label.
    const result = filterSections(SETTINGS_SECTIONS, "sandbox");
    expect(result.map((section) => section.id)).toEqual(["agent"]);
  });

  it("returns nothing when no section matches", () => {
    expect(filterSections(SETTINGS_SECTIONS, "nonexistent-xyz")).toHaveLength(0);
  });
});

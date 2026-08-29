import { describe, expect, it } from "vitest";
import { bannerText, classifyVersion, compareVersions, LAST_STABLE, STABLE } from "$lib/app/codexVersion.svelte";

describe("compareVersions", () => {
  it("compares dotted numbers and ignores a pre-release suffix", () => {
    expect(compareVersions("0.149.1", "0.150.0")).toBe(-1);
    expect(compareVersions("0.150.1", "0.150.1")).toBe(0);
    expect(compareVersions("0.151.0-alpha.7", "0.150.1")).toBe(1);
    expect(compareVersions("1.0", "0.999.999")).toBe(1);
  });
});

describe("classifyVersion", () => {
  it("places versions against the documented tiers", () => {
    expect(classifyVersion(LAST_STABLE)).toBe("supported");
    expect(classifyVersion(STABLE)).toBe("supported");
    expect(classifyVersion("0.146.0")).toBe("older");
    expect(classifyVersion("0.152.0-alpha.1")).toBe("newer");
    // A source build of the mirror reports the workspace version.
    expect(classifyVersion("0.0.0")).toBe("unstable");
  });

  it("wording names the tier boundary the version crossed", () => {
    expect(bannerText({ tier: "older", version: "0.146.0", dismissed: false })).toContain(LAST_STABLE);
    expect(bannerText({ tier: "newer", version: "0.152.0", dismissed: false })).toContain(STABLE);
    expect(bannerText({ tier: "unstable", version: "0.0.0", dismissed: false })).toContain("unreleased");
    expect(bannerText({ tier: "supported", version: STABLE, dismissed: false })).toBe("");
  });
});

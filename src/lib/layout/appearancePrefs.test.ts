import { beforeEach, describe, expect, it } from "vitest";
import { defaultAppearance, loadAppearance, saveAppearance } from "$lib/layout/appearancePrefs.svelte";

beforeEach(() => localStorage.clear());

describe("appearance prefs", () => {
  it("returns defaults when nothing is stored", () => {
    expect(loadAppearance()).toEqual(defaultAppearance());
  });

  it("round-trips density and font size", () => {
    saveAppearance({ density: "compact", fontSize: 18 });
    expect(loadAppearance()).toEqual({ density: "compact", fontSize: 18 });
  });

  it("clamps out-of-range font sizes and coerces unknown density", () => {
    localStorage.setItem("pingu-appearance-prefs", JSON.stringify({ density: "weird", fontSize: 999 }));
    const loaded = loadAppearance();
    expect(loaded.density).toBe("comfortable");
    expect(loaded.fontSize).toBe(20);
    expect(localStorage.getItem("pingex-appearance-prefs")).not.toBeNull();
  });
});

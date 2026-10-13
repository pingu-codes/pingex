import { beforeEach, describe, expect, it } from "vitest";
import { applyAppearance, defaultAppearance, loadAppearance, saveAppearance } from "$lib/layout/appearancePrefs.svelte";

beforeEach(() => localStorage.clear());

describe("appearance prefs", () => {
  it("returns defaults when nothing is stored", () => {
    expect(loadAppearance()).toEqual(defaultAppearance());
  });

  it("round-trips every pref", () => {
    const prefs = { theme: "readable", font: "hyperlegible", density: "compact", fontSize: 18 } as const;
    saveAppearance(prefs);
    expect(loadAppearance()).toEqual(prefs);
  });

  it("loads prefs saved before theme and font existed as defaults", () => {
    localStorage.setItem("pingex-appearance-prefs", JSON.stringify({ density: "compact", fontSize: 14 }));
    expect(loadAppearance()).toEqual({ theme: "default", font: "default", density: "compact", fontSize: 14 });
  });

  it("coerces unknown theme and font", () => {
    localStorage.setItem("pingex-appearance-prefs", JSON.stringify({ theme: "neon", font: "comic" }));
    const loaded = loadAppearance();
    expect(loaded.theme).toBe("default");
    expect(loaded.font).toBe("default");
  });

  it("applies theme and font to the document root and clears them", () => {
    const root = document.documentElement;
    applyAppearance({ ...defaultAppearance(), theme: "readable", font: "hyperlegible" });
    expect(root.dataset.theme).toBe("readable");
    expect(root.dataset.font).toBe("hyperlegible");
    applyAppearance(defaultAppearance());
    expect(root.dataset.theme).toBe("cerberus");
    expect(root.dataset.font).toBeUndefined();
  });

  it("clamps out-of-range font sizes and coerces unknown density", () => {
    localStorage.setItem("pingu-appearance-prefs", JSON.stringify({ density: "weird", fontSize: 999 }));
    const loaded = loadAppearance();
    expect(loaded.density).toBe("comfortable");
    expect(loaded.fontSize).toBe(20);
    expect(localStorage.getItem("pingex-appearance-prefs")).not.toBeNull();
  });
});

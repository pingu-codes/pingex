/** Frontend-only appearance preferences (density and base font size),
 * persisted in localStorage and applied to the document root. These are local
 * runtime settings with no Codex equivalent, so they never touch config.toml. */

export type Density = "comfortable" | "compact";

export interface AppearancePrefs {
  density: Density;
  /** Base font size in px applied to the document root. */
  fontSize: number;
}

export const FONT_SIZE_MIN = 12;
export const FONT_SIZE_MAX = 20;
const DEFAULT_FONT_SIZE = 16;
const STORAGE_KEY = "pingex-appearance-prefs";
const LEGACY_STORAGE_KEY = "pingu-appearance-prefs";

export function defaultAppearance(): AppearancePrefs {
  return { density: "comfortable", fontSize: DEFAULT_FONT_SIZE };
}

function clampFontSize(size: number): number {
  if (!Number.isFinite(size)) return DEFAULT_FONT_SIZE;
  return Math.min(FONT_SIZE_MAX, Math.max(FONT_SIZE_MIN, Math.round(size)));
}

export function loadAppearance(): AppearancePrefs {
  try {
    const raw = localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem(LEGACY_STORAGE_KEY);
    if (!raw) return defaultAppearance();
    const parsed = JSON.parse(raw) as Partial<AppearancePrefs>;
    const prefs: AppearancePrefs = {
      density: parsed.density === "compact" ? "compact" : "comfortable",
      fontSize: clampFontSize(parsed.fontSize ?? DEFAULT_FONT_SIZE),
    };
    if (!localStorage.getItem(STORAGE_KEY)) localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
    return prefs;
  } catch {
    return defaultAppearance();
  }
}

export function saveAppearance(prefs: AppearancePrefs): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // Best-effort; the choice still applies for the session.
  }
}

/** Apply appearance prefs to the document root so they take effect app-wide. */
export function applyAppearance(prefs: AppearancePrefs): void {
  if (typeof document === "undefined") return;
  document.documentElement.style.fontSize = `${clampFontSize(prefs.fontSize)}px`;
  document.documentElement.dataset.density = prefs.density;
}

/** Reactive singleton so any view observing `appearance.prefs` re-renders on change. */
class AppearanceStore {
  prefs = $state<AppearancePrefs>(defaultAppearance());

  constructor() {
    this.prefs = loadAppearance();
  }

  set(next: Partial<AppearancePrefs>): void {
    this.prefs = { ...this.prefs, ...next, fontSize: clampFontSize(next.fontSize ?? this.prefs.fontSize) };
    saveAppearance(this.prefs);
    applyAppearance(this.prefs);
  }
}

export const appearance = new AppearanceStore();

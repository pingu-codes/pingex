// A per-viewer on/off choice in localStorage. Storage can be missing or refuse
// a write, and neither matters: the choice still applies until a remount.

export function readFlag(key: string, fallback = false): boolean {
  try {
    const stored = localStorage.getItem(key);
    return stored === null ? fallback : stored === "true";
  } catch {
    return fallback;
  }
}

export function writeFlag(key: string, value: boolean): void {
  try {
    localStorage.setItem(key, String(value));
  } catch {
    // Fine to lose.
  }
}

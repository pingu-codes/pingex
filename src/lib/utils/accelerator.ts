/**
 * Non-printing keys that only make sense as a whole-name accelerator token.
 * Everything else with a single-character `key` is uppercased as-is.
 */
const NAMED_KEYS: Record<string, string> = {
  " ": "Space",
  Spacebar: "Space",
  ArrowUp: "Up",
  ArrowDown: "Down",
  ArrowLeft: "Left",
  ArrowRight: "Right",
  Enter: "Enter",
  Return: "Enter",
  Escape: "Escape",
  Tab: "Tab",
  Backspace: "Backspace",
  Delete: "Delete",
};

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "OS", "AltGraph"]);

function normalizeKey(event: KeyboardEvent): string | null {
  const { key } = event;
  if (MODIFIER_KEYS.has(key)) return null;
  if (key in NAMED_KEYS) return NAMED_KEYS[key];
  // Function keys (F1–F24) and other multi-char named keys pass through.
  if (/^F\d{1,2}$/.test(key)) return key;
  if (key.length === 1) return key.toUpperCase();
  return null;
}

/**
 * Build a Tauri accelerator string (e.g. `CmdOrCtrl+Shift+K`) from a keydown.
 * Returns `null` while only modifiers are held so a recorder can wait for the
 * final key. `metaKey` maps to `CmdOrCtrl` so the same binding works on macOS
 * (Cmd) and elsewhere (Ctrl).
 */
export function acceleratorFromEvent(event: KeyboardEvent): string | null {
  const key = normalizeKey(event);
  if (!key) return null;
  const parts: string[] = [];
  if (event.metaKey) parts.push("CmdOrCtrl");
  if (event.ctrlKey && !event.metaKey) parts.push("Control");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  parts.push(key);
  return parts.join("+");
}

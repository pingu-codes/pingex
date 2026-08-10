/**
 * Tiny shared channel for opening the Settings dialog at a specific section,
 * optionally focusing an integration. Lets deep components (e.g. an MCP
 * tool-call in the thread view) request the Integrations UI without threading
 * callbacks through every layer.
 */
export type SettingsSection = "general" | "integrations";

export const settingsNav = $state<{
  open: boolean;
  section: SettingsSection;
  /** MCP server name to scroll to / highlight when the section opens. */
  focusServer: string | null;
  /** Tool within `focusServer` to scroll to, when the caller knows which one. */
  focusTool: string | null;
  /** Bumped on each request so effects re-run even for a repeat open. */
  nonce: number;
}>({
  open: false,
  section: "general",
  focusServer: null,
  focusTool: null,
  nonce: 0,
});

export function openSettings(
  section: SettingsSection = "general",
  focusServer: string | null = null,
  focusTool: string | null = null,
): void {
  settingsNav.section = section;
  settingsNav.focusServer = focusServer;
  settingsNav.focusTool = focusTool;
  settingsNav.open = true;
  settingsNav.nonce += 1;
}

export function closeSettings(): void {
  settingsNav.open = false;
  settingsNav.focusServer = null;
  settingsNav.focusTool = null;
}

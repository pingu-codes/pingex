/**
 * The message log: an opt-in, in-memory record of the JSON-RPC traffic between
 * this app and the Codex app-server. The switch lives in Advanced settings;
 * the preference is local to this app (localStorage) and is re-applied to the
 * backend at boot, because recording always starts off there.
 */

import { listen } from "@tauri-apps/api/event";
import { appendMessage } from "$lib/layout/messageLog";
import { clearWireLog, readWireLog, setWireLogging } from "$lib/services/api";
import { isTauri } from "$lib/services/tauri";
import type { WireMessage } from "$lib/types";

const STORAGE_KEY = "pingex-message-log-enabled";
const LEGACY_STORAGE_KEY = "pingu-message-log-enabled";

function loadEnabled(): boolean {
  try {
    const value = localStorage.getItem(STORAGE_KEY) ?? localStorage.getItem(LEGACY_STORAGE_KEY);
    if (value !== null && !localStorage.getItem(STORAGE_KEY)) localStorage.setItem(STORAGE_KEY, value);
    return value === "true";
  } catch {
    return false;
  }
}

function saveEnabled(enabled: boolean): void {
  try {
    localStorage.setItem(STORAGE_KEY, String(enabled));
  } catch {
    // Best-effort; the choice still applies for this session.
  }
}

class MessageLogStore {
  enabled = $state(false);
  messages = $state<WireMessage[]>([]);
  error = $state<string | null>(null);
  private listening = false;

  /** Re-apply the saved preference at boot and start the event stream. */
  async start(): Promise<void> {
    const enabled = loadEnabled();
    if (!enabled) return;
    await this.setEnabled(true);
  }

  async setEnabled(enabled: boolean): Promise<void> {
    this.enabled = enabled;
    saveEnabled(enabled);
    this.error = null;
    if (!enabled) this.messages = [];
    try {
      await setWireLogging(enabled);
      if (enabled) {
        await this.listenOnce();
        await this.refresh();
      }
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  /** Pull the backend buffer, which holds traffic from before this view opened. */
  async refresh(): Promise<void> {
    try {
      this.messages = await readWireLog();
    } catch (cause) {
      this.error = cause instanceof Error ? cause.message : String(cause);
    }
  }

  async clear(): Promise<void> {
    this.messages = [];
    try {
      await clearWireLog();
    } catch {
      // Clearing the view is what the user asked for; a failed backend clear
      // only means the next refresh brings the old messages back.
    }
  }

  /** Subscribe to live messages. The backend stops emitting when recording is
   * off, so the listener is attached once and left in place. */
  private async listenOnce(): Promise<void> {
    if (this.listening || !isTauri()) return;
    this.listening = true;
    await listen<WireMessage>("codex:wire", (event) => {
      if (!this.enabled) return;
      this.messages = appendMessage(this.messages, event.payload);
    });
  }
}

export const messageLog = new MessageLogStore();

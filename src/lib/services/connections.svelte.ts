import { listConnections, refreshConnections } from "$lib/services/api";
import { setThreadHandler } from "$lib/services/codexEvents.svelte";
import type { RemoteConnection } from "$lib/types";

/**
 * Shared, app-wide view of paired remote connections. Backs both the
 * Connections settings page and the persistent app-shell indicator, so a
 * rename/revoke in one place immediately reflects in the other.
 */
export const remoteConnections = $state<{ list: RemoteConnection[]; loaded: boolean }>({
  list: [],
  loaded: false,
});

export function setConnections(list: RemoteConnection[]): void {
  remoteConnections.list = list;
  remoteConnections.loaded = true;
}

export async function loadConnections(): Promise<void> {
  try {
    setConnections(await listConnections());
  } catch {
    // The relay may be offline; keep the last-known list rather than blanking
    // the indicator.
  }
}

export async function reloadConnections(): Promise<RemoteConnection[]> {
  const list = await refreshConnections();
  setConnections(list);
  return list;
}

let started = false;

/**
 * Load connections once and refresh whenever the relay announces a
 * status change (`remoteControl/status/changed`). Safe to call repeatedly.
 */
export function startConnectionsWatch(): void {
  if (started) return;
  started = true;
  loadConnections();
  setThreadHandler((event) => {
    if (event.method === "remoteControl/status/changed") {
      loadConnections();
    }
  });
}

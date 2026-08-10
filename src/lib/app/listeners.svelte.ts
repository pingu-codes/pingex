/**
 * Long-lived subscriptions the app shell starts once at boot: Codex events,
 * remote connections, external links and the Tauri deep-link/quick-chat
 * channels.
 */
import { listen } from "@tauri-apps/api/event";
import { quietRefresh } from "$lib/app/appData.svelte";
import { applyHandoff } from "$lib/app/handoff.svelte";
import { openThreadById, setView, view } from "$lib/app/navigation.svelte";
import { messageLog } from "$lib/layout/messageLogPrefs.svelte";
import { isTauri } from "$lib/services/api";
import { setThreadHandler, startCodexListeners } from "$lib/services/codexEvents.svelte";
import { startConnectionsWatch } from "$lib/services/connections.svelte";
import type { HandoffOpen } from "$lib/types";
import { installExternalLinkHandler } from "$lib/utils/externalLinks";
import { refreshGitStatus } from "$lib/worktrees/gitStatus.svelte";

// Quick-chat handoff: when the floating composer's "Open full thread" fires,
// the backend focuses this window and emits the thread to navigate to.
async function openThreadFromQuick(threadId: string) {
  await quietRefresh();
  openThreadById(threadId);
  // The new thread has not reached the sidebar yet; open it directly so the
  // thread view can resume it.
  if (view.threadId !== threadId) setView({ threadId, projectPath: view.projectPath });
}

export function startApp(): void {
  startCodexListeners();
  // Open links (markdown output, etc.) in the default browser rather than
  // navigating the app webview and stranding the user with no way back.
  installExternalLinkHandler();
  // Track paired remote devices so the sidebar can show a live connection
  // indicator and refresh it when the relay announces status changes.
  startConnectionsWatch();
  // Recording always starts off in the backend, so re-apply the saved
  // Advanced → message log preference.
  void messageLog.start();

  // Threads can be created or renamed outside this window (e.g. from a paired
  // phone via remote control); refresh the sidebar when Codex announces them.
  let sidebarRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  setThreadHandler((event) => {
    // Refresh the active repo's branch chip after a completed agent turn — an
    // explicit, one-shot refresh rather than continuous polling.
    if (event.method === "turn/completed" && view.projectPath) {
      refreshGitStatus(view.projectPath);
    }
    // turn/completed also refreshes the sidebar: a brand-new thread only becomes
    // visible to thread/list once its rollout is persisted, which happens after
    // the thread/started refresh has already run.
    if (
      event.method !== "thread/started" &&
      event.method !== "thread/name/updated" &&
      event.method !== "turn/completed"
    )
      return;
    if (sidebarRefreshTimer) clearTimeout(sidebarRefreshTimer);
    sidebarRefreshTimer = setTimeout(() => {
      sidebarRefreshTimer = null;
      quietRefresh();
    }, 300);
  });

  if (!isTauri()) return;
  // A `codex://` deep link received by the backend (CLI-to-desktop handoff).
  listen<HandoffOpen>("handoff://open", (event) => applyHandoff(event.payload));
  listen<{ threadId: string }>("quickchat://open-thread", (event) => {
    const threadId = event.payload?.threadId;
    if (threadId) openThreadFromQuick(threadId);
  });
}

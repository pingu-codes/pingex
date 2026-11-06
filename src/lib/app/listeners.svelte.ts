/**
 * Long-lived subscriptions the app shell starts once at boot: Codex events,
 * remote connections, external links and the Tauri deep-link/quick-chat
 * channels.
 */

import { listen } from "@tauri-apps/api/event";
import { quietRefresh } from "$lib/app/appData.svelte";
import { openFilePreview } from "$lib/app/filePreview";
import { applyHandoff } from "$lib/app/handoff.svelte";
import { eventMatchesHome } from "$lib/app/launch.svelte";
import { openThreadById, setView, view } from "$lib/app/navigation.svelte";
import { messageLog } from "$lib/layout/messageLogPrefs.svelte";
import { setThreadHandler, startCodexListeners } from "$lib/services/codexEvents.svelte";
import { startConnectionsWatch } from "$lib/services/connections.svelte";
import { belongsToProfile, identity, scopedId, setHomeSelection } from "$lib/services/homeRouting";
import { invalidate } from "$lib/services/queryCache.svelte";
import { isTauri } from "$lib/services/tauri";
import { toastError } from "$lib/toaster";
import type { HandoffOpen } from "$lib/types";
import { errorMessage } from "$lib/utils/errors";
import { installExternalLinkHandler } from "$lib/utils/externalLinks";
import { refreshAllGitStatus, refreshGitStatus } from "$lib/worktrees/gitStatus.svelte";

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
  setHomeSelection(() => view);
  startCodexListeners().catch((cause) => toastError(`Could not listen for Codex events: ${errorMessage(cause)}`));
  // Open links (markdown output, etc.) in the default browser rather than
  // navigating the app webview and stranding the user with no way back. A link
  // that names a local file opens the preview modal instead of Finder.
  installExternalLinkHandler({ onFilePath: (ref) => void openFilePreview(ref) });
  // Track paired remote devices so the sidebar can show a live connection
  // indicator and refresh it when the relay announces status changes.
  startConnectionsWatch();
  // Recording always starts off in the backend, so re-apply the saved
  // Advanced → message log preference.
  void messageLog.start();
  // Branches are usually switched in a terminal; refresh every cached repo's
  // git status, and whatever local git state is on screen, when the user
  // comes back to this window.
  const onReturn = () => {
    refreshAllGitStatus();
    invalidate({ kinds: ["local"] });
  };
  window.addEventListener("focus", onReturn);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") onReturn();
  });

  // Threads can be created or renamed outside this window (e.g. from a paired
  // phone via remote control); refresh the sidebar when Codex announces them.
  let sidebarRefreshTimer: ReturnType<typeof setTimeout> | null = null;
  setThreadHandler((event) => {
    // Refresh the active repo's branch chip after a completed agent turn — an
    // explicit, one-shot refresh rather than continuous polling.
    // The agent may have edited, committed or switched branches anywhere.
    if (event.method === "turn/completed") {
      if (view.projectPath) refreshGitStatus(view.projectPath);
      invalidate({ kinds: ["local"] });
    }
    // turn/completed also refreshes the sidebar: a brand-new thread only becomes
    // visible to thread/list once its rollout is persisted, which happens after
    // the thread/started refresh has already run.
    // project/changed and thread/project/updated come from Codex ≥0.149 when
    // another client reorganises the projects this sidebar mirrors; the
    // archive/delete lifecycle and account/updated are how another client's
    // housekeeping (or a plan change) reaches this window.
    if (
      event.method !== "thread/started" &&
      event.method !== "thread/name/updated" &&
      event.method !== "turn/completed" &&
      event.method !== "project/changed" &&
      event.method !== "thread/project/updated" &&
      event.method !== "thread/archived" &&
      event.method !== "thread/unarchived" &&
      event.method !== "thread/deleted" &&
      event.method !== "account/updated"
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
  listen<HandoffOpen>("handoff://open", (event) => applyHandoff(event.payload)).catch(() => {});
  listen<{ threadId: string; codexHome?: string }>("quickchat://open-thread", (event) => {
    // Broadcast to every window; only the ones on the quick chat's home react.
    if (!eventMatchesHome(event.payload?.codexHome)) return;
    const threadId = event.payload?.threadId;
    const key = event.payload.codexHome;
    if (threadId)
      openThreadFromQuick(key && belongsToProfile(key) && !identity(threadId) ? scopedId(key, threadId) : threadId);
  }).catch(() => {});
}

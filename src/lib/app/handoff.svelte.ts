/**
 * Incoming `codex://` deep links (a CLI-to-desktop handoff). A link that names
 * a different Codex home is never followed silently — the user is asked to
 * switch, or told the home is unknown.
 */
import { projectForCwd, projects, quietRefresh } from "$lib/app/appData.svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import { switchToHome } from "$lib/app/launch.svelte";
import { newThreadInDir, setView, view } from "$lib/app/navigation.svelte";
import { identity, localProjectPath, profileHomes, scopedId } from "$lib/services/homeRouting";
import HandoffOpenDialog from "$lib/thread/HandoffOpenDialog.svelte";
import type { HandoffOpen } from "$lib/types";

export const handoff = $state<{
  /** The worktree an incoming handoff asked for, keyed to the thread it opened
   *  so the mismatch banner clears automatically when navigating elsewhere. */
  expectedCwd: string | null;
  expectedThread: string | null;
  error: string | null;
}>({
  expectedCwd: null,
  expectedThread: null,
  error: null,
});

/** The cwd ThreadView should warn about, if it applies to the open thread. */
export function expectedCwdFor(threadId: string | null): string | null {
  return threadId && threadId === handoff.expectedThread ? handoff.expectedCwd : null;
}

/** Navigate to a thread requested by a deep link, keeping the requested cwd so
 *  ThreadView can warn on a mismatch. */
function openHandoffThread(threadId: string, path: string | null) {
  setView({ threadId, projectPath: projectForCwd(path)?.path ?? null });
  handoff.expectedCwd = path;
  handoff.expectedThread = threadId;
  quietRefresh();
}

/** Act on a same-home handoff: open the thread or a new draft in the cwd. */
function navigateHandoff(open: HandoffOpen) {
  const homes = profileHomes();
  const owner = homes.find(
    (home) =>
      home.home.harness === "codex" &&
      (open.requestedHome
        ? home.home.configDir === open.requestedHome &&
          JSON.stringify(home.home.host) === JSON.stringify(open.requestedHost ?? { kind: "native" })
        : !home.homeKey.startsWith("profile:")),
  );
  if (owner)
    open = {
      ...open,
      path: open.path ? localProjectPath(owner.home.host, open.path) : null,
      threadId: open.threadId && !identity(open.threadId) ? scopedId(owner.homeKey, open.threadId) : open.threadId,
    };
  handoff.error = null;
  if (open.kind === "new") {
    handoff.expectedCwd = null;
    newThreadInDir(open.path ?? view.projectPath ?? projects()[0]?.path ?? "");
    return;
  }
  if (open.threadId) openHandoffThread(open.threadId, open.path);
}

/** Entry point for the `handoff://open` event from a received `codex://` link. */
export async function applyHandoff(open: HandoffOpen): Promise<void> {
  const known = profileHomes().some(
    (home) =>
      home.home.harness === "codex" &&
      home.home.configDir === open.requestedHome &&
      JSON.stringify(home.home.host) === JSON.stringify(open.requestedHost ?? { kind: "native" }),
  );
  if (open.homeMatches || known) {
    navigateHandoff(open);
    return;
  }
  // Differs from the running home (or the home is unknown): never silently
  // fall back — ask the user or report the missing home.
  const switched = await openDialog(HandoffOpenDialog, {
    handoff: open,
    submit: async (requested: HandoffOpen) => {
      if (!requested.requestedHome) return;
      await switchToHome(requested.requestedHome, requested.requestedHost ?? null);
    },
  });
  if (switched) navigateHandoff({ ...open, homeMatches: true });
}

/** After "Move to worktree" forks the thread, open the fork. */
export function movedToWorktree(forkedThreadId: string): void {
  handoff.error = null;
  handoff.expectedCwd = null;
  setView({ threadId: forkedThreadId, projectPath: view.projectPath });
  quietRefresh();
}

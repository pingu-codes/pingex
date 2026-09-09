/**
 * The live thread sessions, and the one Codex event subscription that feeds
 * them.
 *
 * Switching threads remounts `ThreadView`; a session outlives that. While a
 * thread has work in flight — a turn running, messages queued, a stream error
 * unshown — its session stays here and keeps applying events, so returning to
 * it shows exactly what it was doing. An idle thread's session is dropped when
 * its view goes: reading it back from Codex next time is cheaper and safer.
 */
import { type CodexEvent, setThreadHandler } from "$lib/services/codexEvents.svelte";
import { threadBelongsToHome } from "$lib/services/homeRouting";
import { threadIdOf } from "$lib/services/turnLifecycle";
import { ThreadSession } from "$lib/thread/threadSession.svelte";

const sessions = new Map<string, ThreadSession>();
// Until creation returns, only its eventual id tells us which Home owns a draft.
const drafts = new Map<ThreadSession, Set<string>>();
let unlisten: (() => void) | null = null;

function listen() {
  if (unlisten) return;
  unlisten = setThreadHandler(route);
}

function route(event: CodexEvent) {
  const { method, params } = event;
  if (method === "disconnected") {
    // Nothing retained can make progress any more, and a reconnected session is
    // the honest source for what actually survived — forget every session no
    // view is holding on to; tell the rest.
    for (const [session, disconnectedHomes] of drafts) {
      if (!session.working()) continue;
      if (event.homeKey) disconnectedHomes.add(event.homeKey);
      else session.disconnected();
    }
    for (const [id, session] of [...sessions]) {
      if (event.homeKey && !threadBelongsToHome(id, event.homeKey)) continue;
      if (session.mounted > 0) session.disconnected();
      else drop(id, session);
    }
    return;
  }
  const id = threadIdOf(event);
  if (!id) return;
  // A subagent reports under its own id; its parent's session shows it.
  if (method === "thread/status/changed") {
    for (const session of sessions.values()) session.setSubagentStatus(id, params.status);
    return;
  }
  if (method === "thread/started") {
    const parent = params.thread?.parentThreadId;
    if (typeof parent === "string") void sessions.get(parent)?.refreshSubagents();
  }
  sessions.get(id)?.handleEvent(event);
}

function drop(id: string, session: ThreadSession) {
  if (sessions.get(id) !== session) return;
  sessions.delete(id);
  session.dispose();
}

function register(id: string, session: ThreadSession) {
  const previous = sessions.get(id);
  if (previous && previous !== session) previous.dispose();
  sessions.set(id, session);
  session.onIdle = () => drop(id, session);
}

/** The session for a thread a view is opening: the retained one when the
 *  thread was left mid-work, otherwise a fresh one that starts loading. */
export function openSession(id: string): ThreadSession {
  listen();
  let session = sessions.get(id);
  if (!session) {
    session = new ThreadSession(id);
    register(id, session);
    void session.load();
  }
  session.mounted++;
  return session;
}

/** A session for a thread that does not exist yet. It joins the registry
 *  once the view creates the thread and `attach`es its id. */
export function draftSession(cwd: string): ThreadSession {
  listen();
  const session = new ThreadSession(null, cwd);
  drafts.set(session, new Set());
  session.onIdle = () => {
    drafts.delete(session);
    session.dispose();
  };
  session.mounted++;
  return session;
}

/** The draft got an id from Codex: run it under that id from here on. */
export function attachSession(session: ThreadSession, id: string): boolean {
  const disconnectedHomes = drafts.get(session);
  drafts.delete(session);
  if (disconnectedHomes && [...disconnectedHomes].some((home) => threadBelongsToHome(id, home))) {
    session.disconnected();
    return false;
  }
  if (!session.attach(id)) return false;
  register(id, session);
  return true;
}

/** The view is going away. The session is kept only while the thread still
 *  has work in flight. */
export function releaseSession(session: ThreadSession): void {
  session.mounted = Math.max(0, session.mounted - 1);
  if (session.mounted > 0 || session.working()) return;
  const id = session.id;
  if (id) drop(id, session);
  else {
    drafts.delete(session);
    session.dispose();
  }
}

/** The retained session for a thread, if any — for callers other than views. */
export function peekSession(id: string): ThreadSession | null {
  return sessions.get(id) ?? null;
}

/** Test seam: forget every session and re-subscribe from scratch. */
export function resetSessions(): void {
  for (const session of drafts.keys()) session.dispose();
  drafts.clear();
  for (const [id, session] of [...sessions]) drop(id, session);
  unlisten?.();
  unlisten = null;
}

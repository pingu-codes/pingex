/**
 * How a thread comes into being from the app: a draft becoming live where the
 * user chose to start it, its first message, an edit becoming a Branch, and a
 * thread started outright with its opening message (plan handoff, quick chat).
 *
 * Every new thread is titled from its opening words and seeded for the namer
 * in exactly one place (`nameFromOpening`), so the sidebar never reads
 * "Untitled thread" for a turn's whole length whichever way the thread began.
 */

import { nameNewThread } from "$lib/app/appData.svelte";
import { linkExpectedThread } from "$lib/review/prLinks.svelte";
import { forkThread, gitWorktreeAdd, type StartedThread, startThread, startTurn } from "$lib/services/api";
import { commands } from "$lib/services/homeRouting";
import { requestAutoName } from "$lib/thread/autoName";
import { messageText, messageTitle } from "$lib/thread/messageText";
import { attachSession, draftSession, openSession, releaseSession } from "$lib/thread/sessions.svelte";
import type { ThreadSession } from "$lib/thread/threadSession.svelte";
import { withWorktreeContext } from "$lib/thread/worktreeContext";
import type {
  BootstrapData,
  GitRepoInfo,
  HarnessKind,
  Turn,
  TurnOptions,
  UserInputPart,
  WorktreeBranchRequest,
  WorktreeContext,
} from "$lib/types";
import { errorMessage } from "$lib/utils/errors";

/** Where a draft's thread will start: the project's own directory, a
 *  temporary worktree made when it starts, or a permanent one made now. */
export type DraftLocation = "project" | "temporary" | "permanent";

/** What a draft reads from the view showing it. Getters, so each is read at
 *  the moment it matters. */
export interface DraftDeps {
  /** The thread on show; null while the view is a draft. */
  threadId: () => string | null;
  cwd: () => string;
  /** The owning project's path; new worktrees branch from its repository. */
  projectPath: () => string;
  /** A virtual workspace: the backend derives a writable hub and its roots. */
  workspaceId: () => string | null;
  /** The session the view shows, which is the one a created thread is announced for. */
  session: () => ThreadSession;
  /** The harness the thread starts on. */
  harness: () => HarnessKind | null;
  /** Whether the first message carries the offered Worktree context. */
  includeWorktreeContext: () => boolean;
  /** The draft became a thread while a view showed it. */
  onCreated: (id: string, cwd: string, inTempWorktree: boolean) => void;
}

/** Title a new thread from its opening message and seed the namer with it:
 *  shown at once, then refined. Only the user's words, never Worktree context. */
export function nameFromOpening(id: string, input: UserInputPart[]): void {
  nameNewThread(id, messageTitle(input));
  const text = messageText(input);
  if (text.trim()) requestAutoName(id, "seed", text);
}

/**
 * A view's unsent draft: where its thread will start, and how it becomes one.
 * Construct it during component init, since it reads the draft's repository
 * and Worktree context in effects. The draft's session (`session()`) creates
 * the thread itself whenever work needs one — the first message, `/review`,
 * `/goal` — so callers never pass creation around.
 */
export class DraftThread {
  location = $state<DraftLocation>("project");
  /** The permanent worktree chosen, or the temporary one made at start. */
  worktreePath = $state("");
  repoInfo = $state<GitRepoInfo | null>(null);
  /** What the draft's own directory is for, when it is a PR or Stack worktree. */
  #context = $state<WorktreeContext | null>(null);
  /** Where new worktrees branch from. */
  readonly repoDir = $derived.by(() => this.repoInfo?.root ?? (this.deps.projectPath() || this.deps.cwd()));
  /** The Worktree context on offer: only when the thread really starts in that worktree. */
  readonly worktreeContext = $derived.by(() =>
    !this.deps.threadId() && !this.deps.workspaceId() && this.location === "project" ? this.#context : null,
  );

  constructor(private readonly deps: DraftDeps) {
    $effect(() => {
      if (deps.threadId()) return;
      const dir = deps.projectPath() || deps.cwd();
      commands
        .gitRepoInfo(dir)
        .then((info) => {
          this.repoInfo = info;
        })
        .catch(() => {
          this.repoInfo = null;
        });
    });
    $effect(() => {
      const dir = deps.cwd();
      if (deps.threadId() || !dir) return;
      commands
        .reviewWorktreeContext(dir)
        .then((context) => {
          if (dir === deps.cwd()) this.#context = context;
        })
        .catch(() => {
          this.#context = null;
        });
    });
  }

  /** A session for the draft, which creates its thread here when work needs one. */
  session(): ThreadSession {
    return draftSession(this.deps.cwd(), (owner) => this.#create(owner));
  }

  /** Start in the project or a temporary worktree. */
  startIn(location: "project" | "temporary"): void {
    this.location = location;
    this.worktreePath = this.deps.cwd();
  }

  /** Make a permanent worktree at `path` and start there. */
  async addWorktree(path: string, branch: WorktreeBranchRequest): Promise<void> {
    await gitWorktreeAdd(this.repoDir, path, branch);
    this.location = "permanent";
    this.worktreePath = path;
  }

  /**
   * Send from the composer. A draft's first message makes the thread, titles
   * it and seeds the namer before the turn is even started — nothing else can
   * title it until its rollout persists — and only then gains the Worktree
   * context. From there the session owns queueing, the optimistic bubble and
   * the turn. Resolves whether the message is accounted for; a draft that could
   * not start reports why on the session and stays a draft.
   */
  async send(session: ThreadSession, input: UserInputPart[], options?: TurnOptions): Promise<boolean> {
    let outgoing = input;
    if (!session.id && !session.activeTurn && !session.starting) {
      session.starting = true;
      // Read before the thread exists: the offer is a draft's.
      const context = this.deps.includeWorktreeContext() ? this.worktreeContext : null;
      try {
        const id = await session.live();
        nameFromOpening(id, input);
        if (context) outgoing = withWorktreeContext(input, context.text);
      } catch (cause) {
        session.streamError = errorMessage(cause);
        return false;
      } finally {
        session.starting = false;
      }
    }
    return session.send(outgoing, options);
  }

  /** Create the thread `owner` drafts, where the user chose, and run it under its id. */
  async #create(owner: ThreadSession): Promise<string> {
    const workspace = this.deps.workspaceId();
    const harness = this.deps.harness();
    const temporary = this.location === "temporary";
    const cwd = await this.#startDir();
    const created = await startThread(cwd, workspace, harness);
    await linkExpectedThread(created.id, cwd);
    if (!attachSession(owner, created.id)) throw new Error("Thread creation was interrupted.");
    if (owner.thread) {
      owner.thread.harness = created.harness ?? null;
      owner.thread.speedTier = created.speedTier;
    }
    if (owner === this.deps.session() && owner.mounted > 0) {
      this.deps.onCreated(created.id, created.cwd ?? cwd, temporary);
    }
    return created.id;
  }

  async #startDir(): Promise<string> {
    if (this.location === "project") return this.deps.cwd();
    if (this.location === "permanent") return this.worktreePath;
    const path = await commands.gitWorktreeAddTemp(this.repoDir);
    this.worktreePath = path;
    return path;
  }
}

/**
 * Edit a past user message as a new Branch: fork the thread strictly before
 * the message's turn, record the Branch (`onRecorded` gets the data that says
 * so), send the edit on the fork with the view's turn options, and record the
 * edit's turn. The fork's session is primed and sent to before this resolves,
 * so the edit is already streaming when a view opens it.
 *
 * Resolves the Branch's thread id, or null when nothing was made: the message
 * cannot be edited right now, or a step failed, which `session` reports. The
 * original thread is never rewound.
 */
export async function editAsBranch(
  session: ThreadSession,
  turn: Turn,
  parts: UserInputPart[],
  onRecorded?: (data: BootstrapData) => void,
): Promise<string | null> {
  const thread = session.thread;
  const parent = session.id;
  if (!thread || !parent || turn.id.startsWith("local-") || session.activeTurn) return null;
  const index = thread.turns.findIndex((candidate) => candidate.id === turn.id);
  if (index === -1) return null;
  session.streamError = null;
  session.starting = true;
  try {
    const forked = await forkThread(parent, turn.id);
    onRecorded?.(await commands.addThreadBranch(parent, forked.id, turn.id, index));
    const fork = openSession(forked.id);
    try {
      await fork.load();
      const sent = await fork.send(parts, session.turnOptions?.());
      if (!sent) throw new Error(fork.streamError ?? "Could not send the edited message");
      const editTurnId = fork.thread?.turns.at(-1)?.id;
      if (editTurnId && !editTurnId.startsWith("local-")) {
        commands.setThreadBranchEditTurn(forked.id, editTurnId).catch(() => {});
      }
    } finally {
      // Retained while its turn runs; the view that opens it takes it over.
      releaseSession(fork);
    }
    return forked.id;
  } catch (cause) {
    session.streamError = errorMessage(cause);
    return null;
  } finally {
    session.starting = false;
  }
}

/**
 * Start a thread outright with its opening message — for one that never was a
 * draft on screen, such as a plan implemented from a clean context or a quick
 * chat. `beforeTurn` runs once the thread exists, so its events have somewhere
 * to land; `afterTurn` once the turn is started, before the thread is titled
 * (announce it there, or the title has nothing to land on).
 */
export async function startThreadWith(
  where: { cwd: string; workspaceId?: string | null; harness?: HarnessKind | null },
  input: UserInputPart[],
  {
    options,
    beforeTurn,
    afterTurn,
  }: {
    options?: TurnOptions;
    beforeTurn?: (thread: StartedThread) => void;
    afterTurn?: (thread: StartedThread) => void;
  } = {},
): Promise<StartedThread> {
  const thread = await startThread(where.cwd, where.workspaceId, where.harness);
  beforeTurn?.(thread);
  await startTurn(thread.id, input, options);
  afterTurn?.(thread);
  nameFromOpening(thread.id, input);
  return thread;
}

import { nameNewThread } from "$lib/app/appData.svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
import { getThreadGoal, setThreadGoal, setThreadGoalBudget, setThreadGoalStatus } from "$lib/services/api";
import { commands } from "$lib/services/homeRouting";
import { requestAutoName } from "$lib/thread/autoName";
import GoalBudgetDialog from "$lib/thread/GoalBudgetDialog.svelte";
import { parseTokenBudget } from "$lib/thread/goalBudget";
import { messageTitle } from "$lib/thread/messageText";
import type { ThreadSession } from "$lib/thread/threadSession.svelte";
import { toastError } from "$lib/toaster";
import { errorMessage } from "$lib/utils/errors";
import { formatTokensShort } from "$lib/utils/format";

export interface GoalContext {
  /** The session on show. Each command reads it once and sticks with it, so a
   *  thread switch mid-request cannot land the answer on the wrong thread. */
  session: () => ThreadSession;
  /** The thread id to set a goal on, creating the thread from a draft first. */
  ensureLiveThread: (owner: ThreadSession) => Promise<string>;
  /** Give a typed command line back to the composer after it failed. */
  restoreText: (text: string) => void;
}

const NO_GOAL = "No goal is set — /goal <objective> sets one.";

/** `/goal` and the goal banner's actions, for the thread a view is showing. */
export function goalCommands(context: GoalContext) {
  /** A command that could not run: say why, and give the typed line back. */
  function refuse(session: ThreadSession, typed: string, message: string) {
    session.streamError = message;
    if (typed) context.restoreText(typed);
  }

  /**
   * `/goal` — set, view, cap or clear the goal for a long-running task. With no
   * argument the current goal is shown; `clear` drops it; `budget <tokens>` (or
   * `budget clear`) caps its spend; anything else becomes the new objective.
   *
   * Setting an objective needs no conversation behind it, so — like `/review` —
   * an unsent draft becomes a real thread here: that is what starting a thread
   * with a goal means. Reading and clearing stay draft-only answers, since
   * creating a thread just to report it has no goal would be waste.
   */
  async function run(argument: string, typed = "") {
    const session = context.session();
    const budget = /^budget\b\s*(.*)$/i.exec(argument.trim());
    if (budget) {
      await budgetCommand(session, budget[1].trim(), typed);
      return;
    }
    const objective = argument && argument.toLowerCase() !== "clear" ? argument : null;
    const current = session.id;
    if (!objective) {
      // Reading or clearing a goal a draft cannot have yet.
      if (!current) {
        session.notice = NO_GOAL;
        return;
      }
      try {
        if (argument) {
          await commands.threadGoalClear(current);
          session.setGoal(null);
          session.notice = "Goal cleared.";
        } else {
          const held = await getThreadGoal(current);
          session.setGoal(held);
          session.notice = held ? `Goal (${held.status}): ${held.objective}` : NO_GOAL;
        }
      } catch (cause) {
        refuse(session, typed, errorMessage(cause));
      }
      return;
    }
    const ok = await set(objective);
    if (!ok && typed) context.restoreText(typed);
  }

  /** `/goal budget <tokens|clear>` — cap the goal's spend, or lift the cap. */
  async function budgetCommand(session: ThreadSession, argument: string, typed: string) {
    const goal = session.goal;
    if (!session.id || !goal) {
      session.notice = "No goal is set — /goal <objective> sets one first.";
      return;
    }
    if (!argument) {
      session.notice = goal.tokenBudget
        ? `Goal budget: ${formatTokensShort(goal.tokensUsed)} of ${formatTokensShort(goal.tokenBudget)} tokens used.`
        : "The goal has no budget — /goal budget 500k caps it.";
      return;
    }
    const tokens = argument.toLowerCase() === "clear" ? null : parseTokenBudget(argument);
    if (tokens === null && argument.toLowerCase() !== "clear") {
      refuse(session, typed, "Budgets are a number of tokens, like 250000, 500k or 2m.");
      return;
    }
    await applyBudget(session, tokens);
  }

  /** Write `tokens` (or no cap) to the goal and reflect the answer. */
  async function applyBudget(session: ThreadSession, tokens: number | null): Promise<boolean> {
    const goal = session.goal;
    if (!session.id || !goal) return false;
    try {
      const updated = await setThreadGoalBudget(session.id, tokens);
      session.setGoal({ ...goal, ...updated, objective: updated.objective || goal.objective });
      session.notice =
        tokens === null ? "Goal budget lifted." : `Goal budget set: ${formatTokensShort(tokens)} tokens.`;
      return true;
    } catch (cause) {
      toastError(errorMessage(cause));
      return false;
    }
  }

  /** The banner's budget control: cap, raise or lift the goal's token budget. */
  async function editBudget() {
    const session = context.session();
    const goal = session.goal;
    if (!goal) return;
    const chosen = await openDialog(GoalBudgetDialog, { current: goal.tokenBudget, used: goal.tokensUsed });
    if (chosen === undefined) return;
    await applyBudget(session, chosen);
  }

  /**
   * Set `objective` as the thread's goal — from `/goal`, the composer's goal
   * mode, or the banner's edit. Resolves false when nothing was set: the user
   * declined to replace the current goal, or Codex refused. `edit` is the
   * banner's own edit of the visible goal, which needs no replace confirmation.
   */
  async function set(objective: string, edit = false): Promise<boolean> {
    const session = context.session();
    const current = session.id;
    const fresh = !current;
    if (fresh && (session.activeTurn || session.starting)) {
      session.streamError = "A goal can't start a thread while Codex is working.";
      return false;
    }
    if (!fresh && !edit) {
      // The thread may already be working towards something — never overwrite
      // it silently. (This also covers the goal-started draft, which still
      // looks like a draft because adopting the thread does not remount.)
      const existing = session.goal ?? (await getThreadGoal(current).catch(() => null));
      if (existing && existing.objective !== objective) {
        const ok = await openDialog(ConfirmDialog, {
          title: "Replace goal",
          body: [`This thread already has a goal: “${existing.objective}”`, `Replace it with “${objective}”?`],
          confirmLabel: "Replace goal",
          tone: "warning" as const,
        });
        if (!ok) return false;
      }
    }
    if (fresh) session.starting = true;
    try {
      const id = await context.ensureLiveThread(session);
      // No notice: the goal banner confirms it, and the goal's first turn would
      // clear a notice the moment it appeared.
      session.setGoal(await setThreadGoal(id, objective));
      // A goal-only thread has no turn to name it from, so the objective titles
      // it: shown at once, then refined by the namer.
      if (fresh) {
        nameNewThread(id, messageTitle([{ type: "text", text: objective }]));
        requestAutoName(id, "seed", objective);
      }
      return true;
    } catch (cause) {
      session.streamError = errorMessage(cause);
      return false;
    } finally {
      if (fresh) session.starting = false;
    }
  }

  /** Pause a running goal, or resume a paused (or otherwise stalled) one. */
  async function toggle() {
    const session = context.session();
    const goal = session.goal;
    if (!goal || !session.id) return;
    const status = goal.status === "active" ? "paused" : "active";
    try {
      session.setGoal({ ...goal, ...(await setThreadGoalStatus(session.id, status)), objective: goal.objective });
    } catch (cause) {
      toastError(errorMessage(cause));
    }
  }

  async function clear() {
    const session = context.session();
    if (!session.id) return;
    try {
      await commands.threadGoalClear(session.id);
      session.setGoal(null);
    } catch (cause) {
      toastError(errorMessage(cause));
    }
  }

  return { run, set, editBudget, toggle, clear };
}

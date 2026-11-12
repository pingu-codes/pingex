/**
 * A side question's conversation: the questions asked about one parent thread
 * outside its main conversation, each answered on its own.
 *
 * The panel lays one out through this interface and never learns how the
 * answers are produced. Two adapters sit behind it, chosen per parent by
 * `sideQuestionMode`:
 *
 * - fork (`forkSideConversation`): a hidden fork of the parent runs each
 *   question as an ordinary turn, through the same `ThreadSession` a thread
 *   view uses. It can use tools, so its transcript is turns.
 * - native (`nativeSideConversation`): the harness answers in place from what
 *   the parent already knows, without tools. Its transcript is
 *   question-and-answer exchanges.
 */

import { ForkSideConversation } from "$lib/panels/forkSideConversation.svelte";
import { NativeSideConversation } from "$lib/panels/nativeSideConversation.svelte";
import { commands } from "$lib/services/homeRouting";
import type { BootstrapData, SideExchange, SideQuestion, ThreadDetail, Turn } from "$lib/types";

/** What an open side question shows. Its shape is the adapter's: a fork runs
 *  turns with tools; a native answer is text. */
export type SideTranscript =
  | {
      shape: "turns";
      /** The fork thread. Cards answered in the panel write into its turns. */
      thread: ThreadDetail;
      /** The side conversation's own turns, without those it inherited. */
      turns: Turn[];
      activeTurn: Turn | null;
    }
  | { shape: "exchanges"; exchanges: SideExchange[] };

export interface SideConversation {
  /** The parent thread's side questions this conversation can open. */
  readonly entries: SideQuestion[];
  /** The side question on show; `null` shows the list. */
  readonly openId: string | null;
  /** The open side question's transcript, once it has one. */
  readonly transcript: SideTranscript | null;
  /** The open side question's history is being read; asking waits. */
  readonly loading: boolean;
  /** An answer is in progress: asking waits and Stop applies. */
  readonly busy: boolean;
  readonly error: string | null;
  /** What a side question here is. `pinned` keeps it on show while a
   *  conversation is open, for a limit that matters while asking. */
  readonly about: { text: string; pinned: boolean };
  /** Show a side question, or the list for `null`. */
  open(id: string | null): void;
  /** Ask on the open side question, or start a new one from the list.
   *  `false` means the question went nowhere and its text should go back
   *  to the composer. */
  ask(text: string): Promise<boolean>;
  /** Stop the answer in progress. */
  stop(): Promise<void>;
  remove(entry: SideQuestion): Promise<void>;
  /** The panel is done with it. Answers already asked for still land. */
  dispose(): void;
}

/** What a conversation needs from the panel that holds it. */
export interface SideConversationHost {
  /** Every side question the app knows of; the conversation keeps its parent's. */
  sideQuestions: () => SideQuestion[];
  onDataChanged: (data: BootstrapData) => void;
}

/** The side-question conversation for a parent thread, on the adapter its
 *  harness offers. */
export async function openSideConversation(
  parentThreadId: string,
  host: SideConversationHost,
): Promise<SideConversation> {
  const kind = await commands.sideQuestionMode(parentThreadId);
  return kind === "native"
    ? new NativeSideConversation(parentThreadId, host)
    : new ForkSideConversation(parentThreadId, host);
}

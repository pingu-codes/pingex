import { previewEmit, previewEmitServerRequest } from "$lib/services/codexEvents.svelte";
import type { CodexEvent, CodexEventOf } from "$lib/types";
import { nextPreviewId } from "./fixtures";

const previewInterrupted = new Set<string>();

const PREVIEW_CONTEXT_WINDOW = 272_000;
/** Grows with each preview turn so the context ring visibly fills up. */
let previewContextTokens = 24_000;

function previewTokenUsage(threadId: string, turnId: string, delay: number): void {
  const used = previewContextTokens;
  setTimeout(
    () =>
      previewEmit({
        method: "thread/tokenUsage/updated",
        params: {
          threadId,
          turnId,
          tokenUsage: {
            total: breakdown(Math.round(used * 1.8)),
            last: breakdown(used),
            modelContextWindow: PREVIEW_CONTEXT_WINDOW,
          },
        },
      }),
    delay,
  );
}

function breakdown(totalTokens: number) {
  const outputTokens = Math.round(totalTokens * 0.12);
  return {
    totalTokens,
    inputTokens: totalTokens - outputTokens,
    cachedInputTokens: Math.round((totalTokens - outputTokens) * 0.6),
    cacheWriteInputTokens: 0,
    outputTokens,
    reasoningOutputTokens: Math.round(outputTokens * 0.4),
  };
}

export function previewStreamTurn(threadId: string, turnId: string, text: string): void {
  const emit = <M extends CodexEvent["method"]>(method: M, params: CodexEventOf<M>["params"], delay: number) =>
    setTimeout(() => {
      if (!previewInterrupted.has(turnId)) previewEmit({ method, params } as CodexEvent);
    }, delay);
  const reasoningId = `preview-reasoning-${nextPreviewId()}`;
  const messageId = `preview-message-${nextPreviewId()}`;
  const reasoningChunks = [
    "**Considering the request** — looking at ",
    "the current code to figure out the smallest change. ",
    "The existing helper style suggests a small standalone function ",
    "with a trailing-call debounce.",
  ];
  const messageChunks = [
    "Here is a preview response streamed ",
    "chunk by chunk so the live-turn UI ",
    "can be developed without Tauri.",
  ];
  let delay = 250;
  emit("turn/started", { threadId, turn: { id: turnId, status: "inProgress", items: [] } }, delay);
  emit("item/started", { threadId, turnId, item: { type: "reasoning", id: reasoningId, summary: [] } }, (delay += 250));
  emit("item/reasoning/summaryPartAdded", { threadId, turnId, itemId: reasoningId, summaryIndex: 0 }, (delay += 100));
  for (const chunk of reasoningChunks) {
    emit(
      "item/reasoning/summaryTextDelta",
      { threadId, turnId, itemId: reasoningId, delta: chunk, summaryIndex: 0 },
      (delay += 350),
    );
  }
  emit(
    "item/completed",
    {
      threadId,
      turnId,
      item: { type: "reasoning", id: reasoningId, summary: [reasoningChunks.join("")] },
    },
    (delay += 300),
  );
  if (text.toLowerCase().includes("approve")) {
    setTimeout(
      () =>
        previewEmitServerRequest({
          requestId: nextPreviewId(),
          method: "item/commandExecution/requestApproval",
          params: {
            threadId,
            turnId,
            itemId: `preview-cmd-${nextPreviewId()}`,
            command: "rm -rf node_modules",
            cwd: "/Users/ciaran/Projects/codex-custom",
            reason: "Command is not in the allowlist",
          },
        }),
      (delay += 400),
    );
    delay += 1200;
  }
  // A slow streamed command, for developing the Processes menu and detail panel.
  if (text.toLowerCase().includes("sleep") || text.toLowerCase().includes("command")) {
    const commandId = `preview-cmd-${nextPreviewId()}`;
    const command = { type: "commandExecution", id: commandId, command: "sleep 8 && echo done", cwd: "/tmp/preview" };
    emit("item/started", { threadId, turnId, item: command }, (delay += 300));
    for (const line of ["starting…\n", "still working…\n", "done\n"]) {
      emit("item/commandExecution/outputDelta", { threadId, turnId, itemId: commandId, delta: line }, (delay += 2500));
    }
    emit(
      "item/completed",
      { threadId, turnId, item: { ...command, status: "completed", exitCode: 0, durationMs: 8000 } },
      (delay += 400),
    );
  }
  if (text.toLowerCase().includes("question")) {
    setTimeout(
      () =>
        previewEmitServerRequest({
          requestId: nextPreviewId(),
          method: "item/tool/requestUserInput",
          params: {
            threadId,
            turnId,
            itemId: `preview-question-${nextPreviewId()}`,
            questions: [
              {
                id: "approach",
                header: "Approach",
                question: "How should I implement the cache layer?",
                options: [
                  { label: "In-memory", description: "Fast, but resets on restart" },
                  { label: "SQLite", description: "Persistent, slightly slower" },
                ],
              },
            ],
          },
        }),
      (delay += 400),
    );
    delay += 1200;
  }
  emit("item/started", { threadId, turnId, item: { type: "agentMessage", id: messageId, text: "" } }, (delay += 500));
  for (const chunk of messageChunks) {
    emit("item/agentMessage/delta", { threadId, turnId, itemId: messageId, delta: chunk }, (delay += 350));
  }
  emit(
    "item/completed",
    { threadId, turnId, item: { type: "agentMessage", id: messageId, text: messageChunks.join("") } },
    (delay += 300),
  );
  previewContextTokens = Math.min(previewContextTokens + 22_000, PREVIEW_CONTEXT_WINDOW);
  previewTokenUsage(threadId, turnId, (delay += 250));
  emit(
    "turn/completed",
    {
      threadId,
      turn: { id: turnId, status: "completed", items: [], durationMs: delay, completedAt: Date.now() / 1000 },
    },
    (delay += 250),
  );
}

/** Fakes a compaction turn: a marker item, then a much emptier context ring. */
export function previewCompact(threadId: string): void {
  const turnId = `preview-compact-${nextPreviewId()}`;
  const itemId = `preview-compaction-${nextPreviewId()}`;
  previewEmit({ method: "turn/started", params: { threadId, turn: { id: turnId, status: "inProgress", items: [] } } });
  setTimeout(() => {
    previewEmit({
      method: "item/completed",
      params: { threadId, turnId, item: { type: "contextCompaction", id: itemId } },
    });
    previewContextTokens = 26_000;
    previewTokenUsage(threadId, turnId, 0);
    previewEmit({
      method: "turn/completed",
      params: { threadId, turn: { id: turnId, status: "completed", items: [], durationMs: 1200 } },
    });
  }, 1200);
}

export function previewInterrupt(threadId: string, turnId: string): void {
  previewInterrupted.add(turnId);
  previewEmit({
    method: "turn/completed",
    params: { threadId, turn: { id: turnId, status: "interrupted", items: [] } },
  });
}

import type { KnownThreadItemType, ThreadItem, ToolOutputPart, Turn, UserInputPart } from "$lib/types";
import { can } from "$lib/utils/capabilities";

export type Segment = { kind: "item"; item: ThreadItem } | { kind: "reasoning"; items: ThreadItem[] };

export function turnSegments(items: ThreadItem[]): Segment[] {
  const segments: Segment[] = [];
  let reasoning: { kind: "reasoning"; items: ThreadItem[] } | null = null;
  for (const item of items) {
    // An item type this app cannot draw leaves a blank gap mid-turn that only
    // disappears once the turn ends and `splitTurn` takes over. Unlike the
    // completed path this checks the type alone: a reasoning item whose text
    // has not arrived yet is exactly what the live "Working…" shimmer is for.
    if (!RENDERED_TYPES.has(item.type)) continue;
    if (item.type === "reasoning") {
      if (!reasoning) {
        reasoning = { kind: "reasoning", items: [] };
        segments.push(reasoning);
      }
      reasoning.items.push(item);
    } else {
      reasoning = null;
      segments.push({ kind: "item", item });
    }
  }
  return segments;
}

function outputParts(item: ThreadItem): ToolOutputPart[] {
  if (Array.isArray(item.output)) return item.output;
  return item.contentItems ?? [];
}

/**
 * The text of a tool result: a `functionCallOutput` item's `output` (a string
 * or content parts) or a Codex `dynamicToolCall`'s `contentItems`. Images are
 * left to `toolOutputImages`.
 */
export function functionCallOutputText(item: ThreadItem): string {
  if (typeof item.output === "string") return item.output;
  return outputParts(item)
    .filter((part) => typeof part.text === "string")
    .map((part) => part.text as string)
    .join("\n");
}

/** The images in a tool result, as inline `data:image/` URLs. Remote URLs
 *  are left out; Codex refuses them in tool results anyway. */
export function toolOutputImages(item: ThreadItem): string[] {
  return outputParts(item)
    .map((part) => part.imageUrl ?? part.image_url ?? "")
    .filter((url) => url.startsWith("data:image/"));
}

export const segmentKey = (segment: Segment) =>
  segment.kind === "item" ? segment.item.id : `reasoning-${segment.items[0]?.id}`;

export type CompletedSegment =
  | { kind: "message"; item: ThreadItem }
  | { kind: "work"; items: ThreadItem[]; collapsed?: boolean };

/**
 * Whether `WorkItem.svelte` draws each known item type. Typed as a complete
 * record on purpose: adding a type to `THREAD_ITEM_TYPES` without deciding how
 * it is drawn fails to compile here rather than silently rendering nothing.
 */
const DRAWN_BY_WORK_ITEM: Record<KnownThreadItemType, boolean> = {
  agentMessage: true,
  plan: true,
  contextCompaction: true,
  functionCallOutput: true,
  userInputAnswered: true,
  hookPrompt: true,
  reasoning: true,
  commandExecution: true,
  fileChange: true,
  collabAgentToolCall: true,
  subAgentActivity: true,
  enteredReviewMode: true,
  exitedReviewMode: true,
  imageView: true,
  imageGeneration: true,
  sleep: true,
  mcpToolCall: true,
  dynamicToolCall: true,
  webSearch: true,
  // Drawn by `UserMessageBubble.svelte`, and siphoned off before either
  // segmenter sees it.
  userMessage: false,
};

const RENDERED_TYPES = new Set(
  Object.entries(DRAWN_BY_WORK_ITEM)
    .filter(([, drawn]) => drawn)
    .map(([type]) => type),
);

/**
 * Whether an item puts anything on screen. A reasoning item can arrive with no
 * summary at all — Codex reports the text only as deltas — and Codex keeps
 * adding item types this app has not caught up with. Either way the item is
 * invisible, and a work run made only of those would otherwise draw a
 * "Worked for Ns" header that expands to an empty box.
 */
export function rendersSomething(item: ThreadItem): boolean {
  if (item.type === "reasoning") {
    // Either half is enough: the raw content draws its own expander even when
    // no summary ever arrived.
    return [...(item.summary ?? []), ...reasoningContent(item)].some((part) => part?.trim());
  }
  return RENDERED_TYPES.has(item.type);
}

/**
 * The model's unabridged reasoning. Codex puts it in `content`, the same field
 * a `userMessage` uses for its parts, so the strings are picked out rather than
 * assumed.
 */
export function reasoningContent(item: ThreadItem): string[] {
  return (item.content ?? []).filter((part): part is string => typeof part === "string");
}

/** The other half of the same field: the parts of a `userMessage`. */
export function messageParts(item: ThreadItem): UserInputPart[] {
  return (item.content ?? []).filter((part): part is UserInputPart => typeof part !== "string");
}

// Completed turns collapse runs of work (reasoning, commands, file changes)
// into "Worked for Ns" sections, but agent messages, plans and compaction
// markers stay visible — including preambles sent before tool calls, not just
// the final answer.
export function splitTurn(turn: Turn, harness?: string | null) {
  const users: ThreadItem[] = [];
  const body: CompletedSegment[] = [];
  let work: { kind: "work"; items: ThreadItem[] } | null = null;
  for (const item of turn.items) {
    if (item.type === "userMessage") {
      users.push(item);
    } else if (
      item.type === "agentMessage" ||
      item.type === "plan" ||
      item.type === "contextCompaction" ||
      item.type === "userInputAnswered"
    ) {
      work = null;
      body.push({ kind: "message", item });
    } else if (rendersSomething(item)) {
      if (!work) {
        work = { kind: "work", items: [] };
        body.push(work);
      }
      work.items.push(item);
    }
  }
  // A harness with live tool results reports completion while the turn continues.
  // Fold the finished prefix of the live tail, preserving transcript order and
  // leaving running tools (including parallel calls) on screen.
  const tail = body.at(-1);
  if (can(harness, "liveToolResults") && turn.status === "inProgress" && tail?.kind === "work") {
    const firstLive = tail.items.findIndex((item, index) =>
      item.type === "reasoning"
        ? index === tail.items.length - 1
        : !["completed", "failed", "declined", "cancelled", "interrupted"].includes(item.status ?? ""),
    );
    const completedCount = firstLive === -1 ? tail.items.length : firstLive;
    if (completedCount > 0) {
      body[body.length - 1] = { kind: "work", items: tail.items.slice(0, completedCount), collapsed: true };
      if (completedCount < tail.items.length) {
        body.push({ kind: "work", items: tail.items.slice(completedCount) });
      }
    }
  }
  return { users, body };
}

export const completedSegmentKey = (segment: CompletedSegment) =>
  segment.kind === "message" ? segment.item.id : `work-${segment.items[0]?.id}`;

// Diffs collapse by default once a turn has touched more than one file,
// so long threads don't fill up with expanded patches.
export function turnDiffCount(turn: Turn): number {
  let count = 0;
  for (const item of turn.items) {
    if (item.type === "fileChange") count += item.changes?.length ?? 0;
  }
  return count;
}

export function workedLabel(turn: Turn): string {
  let ms = turn.durationMs;
  if (ms == null && turn.startedAt != null && turn.completedAt != null) {
    ms = (turn.completedAt - turn.startedAt) * 1000;
  }
  if (ms == null) return "Worked";
  if (ms < 1000) return `Worked for ${Math.round(ms)}ms`;
  // Round once, then split, so 59.7s reads "1m 00s" rather than "60s".
  const total = Math.max(1, Math.round(ms / 1000));
  if (total < 60) return `Worked for ${total}s`;
  const seconds = total % 60;
  return `Worked for ${Math.floor(total / 60)}m ${String(seconds).padStart(2, "0")}s`;
}

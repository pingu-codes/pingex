import type { KnownThreadItemType, ThreadItem, Turn, UserInputPart } from "$lib/types";

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

/**
 * The text of a `functionCallOutput` item. Upstream sends either a string or
 * content parts; only the text parts are readable here, so images are counted.
 */
export function functionCallOutputText(item: ThreadItem): string {
  const output = item.output;
  if (typeof output === "string") return output;
  if (!Array.isArray(output)) return "";
  const texts = output.filter((part) => typeof part.text === "string").map((part) => part.text as string);
  const images = output.length - texts.length;
  if (images > 0) texts.push(`[${images} image${images === 1 ? "" : "s"}]`);
  return texts.join("\n");
}

export const segmentKey = (segment: Segment) =>
  segment.kind === "item" ? segment.item.id : `reasoning-${segment.items[0]?.id}`;

export type CompletedSegment = { kind: "message"; item: ThreadItem } | { kind: "work"; items: ThreadItem[] };

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
export function splitTurn(turn: Turn) {
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
  if (ms < 60_000) return `Worked for ${Math.max(1, Math.round(ms / 1000))}s`;
  const minutes = Math.floor(ms / 60_000);
  const seconds = Math.round((ms % 60_000) / 1000);
  return `Worked for ${minutes}m ${String(seconds).padStart(2, "0")}s`;
}

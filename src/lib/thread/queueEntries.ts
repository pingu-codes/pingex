import type { QueuedSubmission } from "$lib/types";

/** Queued optimistically while `thread/queue/add` is still in flight. */
const PENDING_PREFIX = "pending-";
/** Queued in this window only, and never going to reach the server: either the
 *  thread does not exist yet, or this Codex has no usable server queue. */
const LOCAL_PREFIX = "local-";

export function pendingId(clientUserMessageId: string): string {
  return `${PENDING_PREFIX}${clientUserMessageId}`;
}

export function localId(clientUserMessageId: string): string {
  return `${LOCAL_PREFIX}${clientUserMessageId}`;
}

/** Whether this entry only exists in the browser, so the server has nothing to
 *  delete for it and a re-list must not drop it. */
export function isClientQueued(entry: QueuedSubmission): boolean {
  return entry.id.startsWith(PENDING_PREFIX) || entry.id.startsWith(LOCAL_PREFIX);
}

/** Whether this entry is known never to reach the server, so the UI can say so. */
export function isLocalOnly(entry: QueuedSubmission): boolean {
  return entry.id.startsWith(LOCAL_PREFIX);
}

/** Fold a fresh server listing into what this window is holding, keeping the
 *  client-only entries the server does not know about. Server order wins; a
 *  client entry the server has since acknowledged is dropped rather than
 *  duplicated, matched on `clientUserMessageId` because the ids differ. */
export function mergeQueue(serverItems: QueuedSubmission[], current: QueuedSubmission[]): QueuedSubmission[] {
  const acknowledged = new Set(serverItems.map((item) => item.clientUserMessageId));
  return [
    ...serverItems,
    ...current.filter((entry) => isClientQueued(entry) && !acknowledged.has(entry.clientUserMessageId)),
  ];
}

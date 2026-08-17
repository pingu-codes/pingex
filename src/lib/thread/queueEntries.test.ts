import { describe, expect, it } from "vitest";
import { isClientQueued, isLocalOnly, localId, mergeQueue, pendingId } from "$lib/thread/queueEntries";
import type { QueuedSubmission } from "$lib/types";

function entry(id: string, clientUserMessageId: string, text = "hi"): QueuedSubmission {
  return { id, input: [{ type: "text", text }], clientUserMessageId };
}

describe("queue entry identity", () => {
  it("treats both optimistic and local-only entries as client-held", () => {
    expect(isClientQueued(entry(pendingId("c1"), "c1"))).toBe(true);
    expect(isClientQueued(entry(localId("c1"), "c1"))).toBe(true);
    expect(isClientQueued(entry("srv-1", "c1"))).toBe(false);
  });

  it("only calls an entry local-only once it can never reach the server", () => {
    expect(isLocalOnly(entry(localId("c1"), "c1"))).toBe(true);
    expect(isLocalOnly(entry(pendingId("c1"), "c1"))).toBe(false);
    expect(isLocalOnly(entry("srv-1", "c1"))).toBe(false);
  });
});

describe("mergeQueue", () => {
  it("keeps a local-only entry the server has never heard of", () => {
    const merged = mergeQueue([entry("srv-1", "c1")], [entry(localId("c2"), "c2")]);
    expect(merged.map((item) => item.id)).toEqual(["srv-1", localId("c2")]);
  });

  it("drops a client entry once the server acknowledges the same message", () => {
    // The ids differ — the server assigns its own — so this has to match on
    // clientUserMessageId or the message renders twice.
    const merged = mergeQueue([entry("srv-1", "c1")], [entry(pendingId("c1"), "c1")]);
    expect(merged).toHaveLength(1);
    expect(merged[0].id).toBe("srv-1");
  });

  it("preserves server order and puts client-only entries last", () => {
    const merged = mergeQueue(
      [entry("srv-1", "c1"), entry("srv-2", "c2")],
      [entry(localId("c3"), "c3"), entry("srv-2", "c2")],
    );
    expect(merged.map((item) => item.id)).toEqual(["srv-1", "srv-2", localId("c3")]);
  });

  it("does not resurrect a server entry that has since been started or deleted", () => {
    expect(mergeQueue([], [entry("srv-1", "c1")])).toEqual([]);
  });

  it("falls back to the client's own entries when the server reports nothing", () => {
    // The unsupported case: queueList rejects, so nothing ever calls this — but
    // an empty successful listing must not wipe a local queue either.
    const local = [entry(localId("c1"), "c1"), entry(localId("c2"), "c2")];
    expect(mergeQueue([], local).map((item) => item.id)).toEqual([localId("c1"), localId("c2")]);
  });
});

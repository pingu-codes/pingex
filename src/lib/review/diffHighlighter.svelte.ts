// Highlighted diff rows for the review page, computed in a Web Worker
// (`highlight.worker.ts`) so neither opening a large PR nor scrolling through
// it waits on highlight.js. A file shows plain until its rows come back.
//
// Files are asked for as their rows mount (near the viewport), and the worker
// takes the newest request first: after a fast scroll, the files now on
// screen go ahead of those scrolled past. Without Worker (tests), rows are
// computed in place.

import type { PrFile } from "$lib/types";
import { highlightedRows } from "./diffHighlight";
import type { HighlightRequest, HighlightResponse } from "./highlight.worker";

/** Done: the rows, or `null` for plain text. Pending files are absent. */
const results = new WeakMap<PrFile, Map<string, string> | null>();
/** The same by path and patch text: a PR re-read after each write brings new
 *  file objects, and an unchanged file should not flash plain while it is
 *  highlighted again. Bounded, oldest first. */
const byContent = new Map<string, Map<string, string> | null>();
const BY_CONTENT_ENTRIES = 500;
const contentKey = (file: PrFile) => (file.patch ? `${file.path}\u0000${file.patch}` : null);

function remember(file: PrFile, rows: Map<string, string> | null) {
  results.set(file, rows);
  const key = contentKey(file);
  if (key === null) return;
  if (byContent.size >= BY_CONTENT_ENTRIES) byContent.delete(byContent.keys().next().value as string);
  byContent.set(key, rows);
}
const requested = new WeakSet<PrFile>();
/** Bumped as each result lands, so readers look again. */
let landed = $state(0);

let worker: Worker | null | undefined;
const queue: PrFile[] = [];
const inFlight = new Map<number, PrFile>();
let nextId = 0;

function startWorker(): Worker | null {
  if (worker !== undefined) return worker;
  try {
    worker =
      typeof Worker === "undefined"
        ? null
        : new Worker(new URL("./highlight.worker.ts", import.meta.url), { type: "module" });
  } catch {
    worker = null;
  }
  if (worker) {
    worker.onmessage = ({ data }: MessageEvent<HighlightResponse>) => {
      const file = inFlight.get(data.id);
      inFlight.delete(data.id);
      if (file) remember(file, data.rows);
      landed++;
      send();
    };
    worker.onerror = () => {
      // Leave every file plain rather than highlight on the main thread.
      worker?.terminate();
      worker = null;
      for (const file of [...inFlight.values(), ...queue.splice(0)]) results.set(file, null);
      inFlight.clear();
      landed++;
    };
  }
  return worker;
}

/** One request at a time, so the newest one always goes next. */
function send() {
  if (!worker || inFlight.size > 0) return;
  const file = queue.pop();
  if (!file) return;
  const id = nextId++;
  inFlight.set(id, file);
  // A snapshot: a file from `$state` is a proxy, which can't be posted.
  const request: HighlightRequest = {
    id,
    file: { path: file.path, hunks: $state.snapshot(file.hunks), patchTruncated: file.patchTruncated },
  };
  worker.postMessage(request);
}

/**
 * `file`'s highlighted rows (see `highlightedRows`), or `null` while they are
 * computed or when it shows plain. Reactive: reading it asks for the rows,
 * and readers update when they land.
 */
export function highlightFor(file: PrFile): Map<string, string> | null {
  void landed;
  if (results.has(file)) return results.get(file) ?? null;
  const key = contentKey(file);
  if (key !== null && byContent.has(key)) {
    const rows = byContent.get(key) ?? null;
    results.set(file, rows);
    return rows;
  }
  if (!startWorker()) {
    const rows = highlightedRows(file);
    remember(file, rows);
    return rows;
  }
  if (!requested.has(file)) {
    requested.add(file);
    queue.push(file);
    queueMicrotask(send);
  }
  return null;
}

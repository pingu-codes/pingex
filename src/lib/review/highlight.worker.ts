// Highlights review diffs off the main thread: highlight.js on a large file
// takes tens of milliseconds, a dropped frame or several if it ran on scroll.
// See `diffHighlighter.svelte.ts`, which sends the requests.

import { computeRows, type HighlightSource } from "./diffHighlight";

export type HighlightRequest = { id: number; file: HighlightSource };
export type HighlightResponse = { id: number; rows: Map<string, string> | null };

const scope = self as unknown as {
  onmessage: ((event: MessageEvent<HighlightRequest>) => void) | null;
  postMessage(message: HighlightResponse): void;
};

scope.onmessage = ({ data }) => {
  let rows: Map<string, string> | null = null;
  try {
    rows = computeRows(data.file);
  } catch {
    // A grammar that throws leaves the file plain.
  }
  scope.postMessage({ id: data.id, rows });
};

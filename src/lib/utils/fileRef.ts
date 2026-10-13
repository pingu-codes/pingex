/**
 * Splitting an agent-emitted file reference into a path and the line it cites.
 *
 * Agents write `src/lib/review/review.test.ts:100` constantly, and everything
 * downstream — the preview modal, `reveal_in_finder` — needs the bare path. The
 * backend cannot do this split: a real filename may contain a colon, and only
 * the frontend knows the string came from a link label rather than from disk.
 */

/** A path with the line and column an agent cited, when it cited any. */
export type FileRef = { path: string; line: number | null; column: number | null };

/** Past this a "line number" is a hash or a timestamp, not a line. */
const MAX_LINE = 10_000_000;

/**
 * Split `path[:line[:column]]`. Percent-decodes first, since the reference
 * arrives as an anchor `href`.
 *
 * Suffixes are popped right to left, at most twice, rather than matched with
 * one greedy regex — that is what keeps `C:\Users\x\a.rs:100` intact, and what
 * stops `C:100` from being read as drive `C:` at line 100. A path that really
 * does end in `:<digits>` is indistinguishable from a citation, so the numeric
 * reading wins; `/srv/log:2024:99` reads as line 2024, column 99.
 */
export function parseFileRef(href: string): FileRef {
  let path = safeDecode(href);
  const numbers: number[] = [];
  while (numbers.length < 2) {
    const match = /^(.*):(\d+)$/.exec(path);
    if (!match) break;
    const [, head, digits] = match;
    // A single-letter head means the colon we just ate was a Windows drive
    // separator, not a citation; a trailing `:` means we ate one too many.
    if (!head || head.endsWith(":") || /^[A-Za-z]$/.test(head)) break;
    numbers.push(Number(digits));
    path = head;
  }
  numbers.reverse();
  return {
    path,
    line: lineOrNull(numbers[0]),
    column: lineOrNull(numbers[1]),
  };
}

/** `:0` is how agents spell "the whole file"; treat it as no citation. */
function lineOrNull(value: number | undefined): number | null {
  if (value === undefined || value <= 0 || value > MAX_LINE) return null;
  return value;
}

/** `decodeURIComponent` that leaves a malformed escape alone. */
function safeDecode(path: string): string {
  try {
    return decodeURIComponent(path);
  } catch {
    return path;
  }
}

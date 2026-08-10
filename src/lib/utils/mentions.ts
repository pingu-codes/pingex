/**
 * Codex does not round-trip `mention` input parts: once a turn is persisted, a
 * file mention comes back as markdown-link text (`[index.ts](src/index.ts)`) in
 * a plain text part. These helpers recover the mention so history renders the
 * same chips the composer showed when the message was sent.
 */

export type MentionSegment = { type: "text"; text: string } | { type: "mention"; name: string; path: string };

const LINK = /\[([^\]\n]+)\]\(([^()\s]+)\)/g;

const basename = (path: string) => path.replace(/\/+$/, "").split("/").pop() ?? path;

/**
 * A markdown link is only treated as a mention when its label is exactly the
 * final path segment — the shape Codex writes. Prose links (`[docs](https://…)`,
 * `[the plan](#plan)`) keep their label and are left as text.
 */
function isMentionLink(label: string, target: string): boolean {
  if (/^[a-z][a-z0-9+.-]*:/i.test(target) || target.startsWith("#")) return false;
  return label === basename(target);
}

/** Splits message text into plain runs and the file mentions embedded in it. */
export function splitMentions(text: string): MentionSegment[] {
  const segments: MentionSegment[] = [];
  let cursor = 0;
  for (const match of text.matchAll(LINK)) {
    const [raw, label, target] = match;
    if (!isMentionLink(label, target)) continue;
    const start = match.index ?? 0;
    if (start > cursor) segments.push({ type: "text", text: text.slice(cursor, start) });
    segments.push({ type: "mention", name: label, path: target });
    cursor = start + raw.length;
  }
  if (cursor < text.length) segments.push({ type: "text", text: text.slice(cursor) });
  return segments;
}

/** True when the text carries at least one mention link. */
export const hasMentions = (text: string): boolean => splitMentions(text).some((segment) => segment.type === "mention");

/** Resolves a mention path against the thread's cwd so it can be revealed. */
export function resolveMentionPath(path: string, cwd: string): string {
  if (path.startsWith("/") || !cwd) return path;
  return `${cwd.replace(/\/+$/, "")}/${path}`;
}

/** Inverse of `resolveMentionPath`: the cwd-relative form Codex itself writes. */
export function relativeMentionPath(path: string, cwd: string): string {
  const root = cwd.replace(/\/+$/, "");
  return root && path.startsWith(`${root}/`) ? path.slice(root.length + 1) : path;
}

/**
 * The `./`-prefixed relative form used when a message is copied to the
 * clipboard, so a pasted `[a.ts](./src/a.ts)` reads unambiguously as a path.
 * Absolute paths are left as they are.
 */
export function copyMentionPath(path: string, cwd: string): string {
  const relative = relativeMentionPath(path, cwd);
  if (relative.startsWith("/") || relative.startsWith("./") || relative.startsWith("../")) return relative;
  return `./${relative}`;
}

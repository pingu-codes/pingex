// Paths reach the frontend from native, WSL and Windows Homes, so these split
// on both `/` and `\`.

/** The path without trailing separators. A bare root (`/`) becomes `""`. */
export function trimSlash(path: string): string {
  return path.replace(/[/\\]+$/, "");
}

/**
 * The final path segment, ignoring trailing separators: `/a/b/` → `b`,
 * `C:\a\b` → `b`. A path with no segment (`""`, `/`) comes back unchanged.
 */
export function basename(path: string): string {
  const trimmed = trimSlash(path);
  const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  return trimmed.slice(cut + 1) || path;
}

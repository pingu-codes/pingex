import type { FileSystemPath, RequestPermissionProfile } from "$lib/types";

/**
 * The filesystem half of a permission profile arrives two ways: as flat
 * `read`/`write` path lists (the shape Codex is moving away from) or as
 * `entries` pairing a path with its access mode. Both may be present on the
 * same request, so entries win and the legacy lists only fill in when there
 * are none.
 */
function describePath(path: FileSystemPath): string {
  return path.path ?? path.pattern ?? path.value ?? "?";
}

/** One line per thing being asked for, ready to list under an approval prompt. */
export function permissionLines(profile: RequestPermissionProfile | undefined): string[] {
  const lines: string[] = [];
  if (profile?.network?.enabled) lines.push("network access");
  const files = profile?.fileSystem;
  if (files?.entries?.length) {
    for (const entry of files.entries) lines.push(`${entry.access} ${describePath(entry.path)}`);
  } else {
    for (const path of files?.read ?? []) lines.push(`read ${path}`);
    for (const path of files?.write ?? []) lines.push(`write ${path}`);
  }
  // A profile that asks for nothing recognisable still needs a decision, so say
  // something rather than showing an empty list above three buttons.
  return lines.length ? lines : ["additional permissions"];
}

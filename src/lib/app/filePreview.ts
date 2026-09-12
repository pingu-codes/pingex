/**
 * Opening the file-preview modal for a path an agent cited.
 *
 * The read happens here rather than inside the dialog: a directory should be
 * revealed rather than previewed, and doing the work first means the dialog is
 * a pure renderer with no loading state of its own.
 */
import { openDialog } from "$lib/app/dialogs.svelte";
import { currentProject, selectedThreadInfo } from "$lib/app/navigation.svelte";
import FilePreviewDialog from "$lib/components/FilePreviewDialog.svelte";
import { readFilePreview, revealInFinder } from "$lib/services/api";
import type { FilePreview } from "$lib/types";
import type { FileRef } from "$lib/utils/fileRef";

/**
 * Preview `ref`, falling back to the file manager for a folder. `cwd` is only
 * needed for the relative form of the path; the document-level link handler has
 * no context to pass, so it is resolved from the current view instead.
 */
export async function openFilePreview(ref: FileRef, cwd?: string | null): Promise<void> {
  if (!ref.path) return;
  const root = cwd ?? currentProject()?.path ?? selectedThreadInfo()?.cwd ?? null;

  let preview: FilePreview;
  try {
    preview = await readFilePreview(ref.path);
  } catch (cause) {
    // The modal still opens and says what went wrong — dropping the click
    // silently is the behaviour this whole change exists to remove.
    preview = {
      path: ref.path,
      text: "",
      bytes: 0,
      truncated: false,
      problem: "unreadable",
      detail: cause instanceof Error ? cause.message : String(cause),
    };
  }

  if (preview.problem === "directory") {
    await revealInFinder(preview.path).catch(() => {});
    return;
  }

  await openDialog(FilePreviewDialog, {
    path: preview.path,
    line: ref.line,
    column: ref.column,
    cwd: root,
    preview,
  });
}

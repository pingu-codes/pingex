import type { Attachment } from "$lib/types";
import { guessKind } from "$lib/utils/media";
import { basename } from "$lib/utils/path";

// Browser/Playwright staging: there is no native filesystem copy here, so we
// fabricate the same metadata shape the Rust `stage_*` commands return. Image
// thumbnails use a blob URL so previews still render without Tauri.

let counter = 0;
const previewId = () => `preview-att-${Date.now().toString(36)}-${counter++}`;

/** Fabricate a staged attachment from a browser `File` (drag/drop, picker). */
export function previewStageFile(file: File): Attachment {
  const kind = guessKind(file.name, file.type);
  return {
    id: previewId(),
    filename: file.name,
    mime: file.type || "application/octet-stream",
    size: file.size,
    stagedPath: kind === "image" ? URL.createObjectURL(file) : `preview://${file.name}`,
    kind,
  };
}

/** Fabricate a staged image from raw bytes (clipboard paste in browser mode). */
export function previewStageBytes(filename: string, mime: string, bytes: number[]): Attachment {
  const blob = new Blob([new Uint8Array(bytes)], { type: mime || "image/png" });
  return {
    id: previewId(),
    filename,
    mime: mime || "image/png",
    size: bytes.length,
    stagedPath: URL.createObjectURL(blob),
    kind: guessKind(filename, mime),
  };
}

/** Fabricate a staged attachment from a native path (never hit in the browser). */
export function previewStageFromPath(sourcePath: string): Attachment {
  const filename = basename(sourcePath);
  return {
    id: previewId(),
    filename,
    mime: "application/octet-stream",
    size: 0,
    stagedPath: sourcePath,
    kind: guessKind(filename, ""),
  };
}

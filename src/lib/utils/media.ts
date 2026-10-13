// Which files count as images. Mirrors `util::media::IMAGE_EXTENSIONS` in
// src-tauri so a placeholder's kind matches what the stage command returns.

export const IMAGE_EXTENSIONS: ReadonlySet<string> = new Set([
  "png",
  "jpg",
  "jpeg",
  "gif",
  "webp",
  "bmp",
  "tif",
  "tiff",
  "heic",
  "avif",
  "ico",
  "svg",
]);

/** Whether `name` (a filename or path) ends in an image extension. */
export function isImagePath(name: string): boolean {
  const base = name.slice(Math.max(name.lastIndexOf("/"), name.lastIndexOf("\\")) + 1);
  const dot = base.lastIndexOf(".");
  return dot > 0 && IMAGE_EXTENSIONS.has(base.slice(dot + 1).toLowerCase());
}

/** Attachment kind from a MIME type when there is one, else the extension. */
export function guessKind(filename: string, mime = ""): "image" | "file" {
  if (mime.startsWith("image/")) return "image";
  return isImagePath(filename) ? "image" : "file";
}

/**
 * Attachment staging for the composer. Files and images enter through the
 * paperclip, drag/drop, or an image paste; each gets an inline chip in the
 * editor that shows staging → ready/failed. Construct it during component
 * init: it registers the native drag/drop listener on mount.
 */
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { onMount } from "svelte";
import type { AttachmentChipHandlers, AttachmentPart } from "$lib/composer/composerParts";
import type { RichEditor } from "$lib/composer/richEditor.svelte";
import { stageBrowserFile } from "$lib/services/api";
import { commands } from "$lib/services/homeRouting";
import { isTauri } from "$lib/services/tauri";
import { guessKind } from "$lib/utils/media";
import { basename } from "$lib/utils/path";

/** What a chip was staged from, kept so a failed one can be retried. */
type StageSource =
  | { via: "path"; path: string }
  | { via: "file"; file: File }
  | { via: "bytes"; filename: string; mime: string; bytes: number[] };

export interface AttachmentStagerDeps {
  /** The editor the chips go into. */
  editor: () => RichEditor;
  /** The drop target: native drops count only over this box. */
  dropZone: () => HTMLElement | null;
  /** Attaching is off (the composer is disabled). */
  disabled: () => boolean;
}

export class AttachmentStager {
  /** A drag is over the drop zone. */
  dragOver = $state(false);
  /** The hidden `<input type=file>` the paperclip opens outside Tauri. */
  fileInput = $state<HTMLInputElement | null>(null);

  /** What each chip was staged from, keyed by the chip's current id. */
  private readonly sources = new Map<string, StageSource>();
  private nextClientId = 0;

  /** Controls on attachment chips, for the editor to wire in. */
  readonly chipHandlers: AttachmentChipHandlers = {
    onRetry: (id) => {
      const source = this.sources.get(id);
      if (source) void this.stage(id, source, false);
    },
    thumbSrc: (part) => {
      if (part.kind !== "image" || !part.path) return null;
      return isTauri() ? convertFileSrc(part.path) : part.path;
    },
  };

  constructor(private readonly deps: AttachmentStagerDeps) {
    // Native (Tauri) file drag/drop. The webview delivers OS file drops with
    // real paths here (HTML drag events are suppressed under Tauri); highlight
    // only when the cursor is over the composer, and stage the dropped paths.
    onMount(() => {
      if (!isTauri()) return;
      let unlisten: (() => void) | undefined;
      let cancelled = false;
      void getCurrentWebview()
        .onDragDropEvent((event) => {
          const payload = event.payload;
          if (payload.type === "enter" || payload.type === "over") {
            this.dragOver = this.overDropZone(payload.position);
          } else if (payload.type === "drop") {
            if (this.overDropZone(payload.position) && !deps.disabled()) this.attachPaths(payload.paths);
            this.dragOver = false;
          } else {
            this.dragOver = false;
          }
        })
        .then((fn) => {
          if (cancelled) fn();
          else unlisten = fn;
        })
        .catch(() => {});
      return () => {
        cancelled = true;
        unlisten?.();
      };
    });
  }

  /** A chip is leaving the editor: unstage the file behind it. */
  chipRemoved(chip: HTMLElement): void {
    const attachmentId = chip.dataset.attachmentId;
    if (!attachmentId) return;
    void commands.removeStaged(attachmentId).catch(() => {});
    this.sources.delete(attachmentId);
  }

  /** The chips left with a sent message: there is nothing left to retry. */
  forget(): void {
    this.sources.clear();
  }

  /** Native (Tauri) or browser file picker behind the paperclip button. */
  async pickFiles(): Promise<void> {
    if (this.deps.disabled()) return;
    if (isTauri()) {
      const picked = await openFileDialog({ multiple: true });
      if (Array.isArray(picked)) this.attachPaths(picked);
      else if (typeof picked === "string") this.attachPaths([picked]);
      return;
    }
    this.fileInput?.click();
  }

  handleFileInputChange = (event: Event): void => {
    const input = event.currentTarget as HTMLInputElement;
    if (input.files) this.attachFiles([...input.files]);
    input.value = "";
  };

  // Browser (Playwright) drag/drop: under Tauri these HTML events are
  // suppressed and the native `onDragDropEvent` listener handles it.
  handleDrop = (event: DragEvent): void => {
    if (isTauri()) return;
    event.preventDefault();
    this.dragOver = false;
    const files = event.dataTransfer?.files;
    if (files?.length) this.attachFiles([...files]);
  };

  handleDragOver = (event: DragEvent): void => {
    if (isTauri()) return;
    event.preventDefault();
    this.dragOver = true;
  };

  handleDragLeave = (): void => {
    if (isTauri()) return;
    this.dragOver = false;
  };

  /** An image paste is staged; anything else goes in as plain text. */
  handlePaste = (event: ClipboardEvent): void => {
    const items = [...(event.clipboardData?.items ?? [])];
    const imageItem = items.find((item) => item.kind === "file" && item.type.startsWith("image/"));
    const file = imageItem?.getAsFile();
    event.preventDefault();
    if (file) {
      void this.stageImageFile(file);
      return;
    }
    document.execCommand("insertText", false, event.clipboardData?.getData("text/plain") ?? "");
  };

  private overDropZone(position: { x: number; y: number }): boolean {
    const box = this.deps.dropZone()?.getBoundingClientRect();
    if (!box) return false;
    const ratio = window.devicePixelRatio || 1;
    const x = position.x / ratio;
    const y = position.y / ratio;
    return x >= box.left && x <= box.right && y >= box.top && y <= box.bottom;
  }

  private attachPaths(paths: string[]): void {
    for (const path of paths) void this.stage(this.clientId(), { via: "path", path }, true);
  }

  private attachFiles(files: File[]): void {
    for (const file of files) void this.stage(this.clientId(), { via: "file", file }, true);
  }

  /** Read a pasted image `File` into bytes for native staging. */
  private async stageImageFile(file: File): Promise<void> {
    if (!isTauri()) {
      this.attachFiles([file]);
      return;
    }
    const bytes = [...new Uint8Array(await file.arrayBuffer())];
    const filename = file.name || "pasted-image.png";
    void this.stage(this.clientId(), { via: "bytes", filename, mime: file.type || "image/png", bytes }, true);
  }

  private clientId(): string {
    return `c${this.nextClientId++}`;
  }

  /** Insert (or, on retry, refresh) an attachment chip and run its staging. */
  private async stage(clientId: string, source: StageSource, insert: boolean): Promise<void> {
    const editor = this.deps.editor();
    if (!editor.element) return;
    this.sources.set(clientId, source);
    const placeholder = placeholderFor(clientId, source);
    if (insert) editor.insertAttachment(placeholder);
    else editor.updateAttachment(clientId, placeholder);
    try {
      const staged =
        source.via === "path"
          ? await commands.stageAttachment(source.path)
          : source.via === "file"
            ? await stageBrowserFile(source.file)
            : await commands.stageClipboardImage(source.filename, source.mime, source.bytes);
      const ready: AttachmentPart = {
        type: "attachment",
        id: staged.id,
        filename: staged.filename,
        mime: staged.mime,
        size: staged.size,
        path: staged.stagedPath,
        kind: staged.kind as AttachmentPart["kind"],
        state: "ready",
      };
      editor.updateAttachment(clientId, ready);
      this.sources.delete(clientId);
      this.sources.set(staged.id, source);
    } catch {
      editor.updateAttachment(clientId, { ...placeholder, state: "failed" });
    }
  }
}

function placeholderFor(clientId: string, source: StageSource): AttachmentPart {
  const filename =
    source.via === "path" ? basename(source.path) : source.via === "file" ? source.file.name : source.filename;
  const mime = source.via === "bytes" ? source.mime : source.via === "file" ? source.file.type : "";
  const size = source.via === "file" ? source.file.size : source.via === "bytes" ? source.bytes.length : 0;
  return {
    type: "attachment",
    id: clientId,
    filename,
    mime,
    size,
    path: "",
    kind: guessKind(filename, mime),
    state: "staging",
  };
}

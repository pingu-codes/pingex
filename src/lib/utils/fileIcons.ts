import {
  Braces,
  Database,
  File,
  FileCode,
  FileImage,
  FileText,
  FileType,
  Folder,
  Package,
  Settings2,
  TerminalSquare,
} from "@lucide/svelte";
import type { Component } from "svelte";

interface FileIcon {
  icon: Component<{ size?: number; class?: string }>;
  class: string;
  /**
   * The Lucide glyph's inner SVG, for call sites that build DOM by hand
   * (composer chips) and so cannot render the component. Kept beside the
   * component so the two can never drift apart.
   */
  body: string;
}

const FILE_OUTLINE =
  '<path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"/><path d="M14 2v5a1 1 0 0 0 1 1h5"/>';

/** Component/markup pairs, each mirroring one `@lucide/svelte` icon exactly. */
const glyphs = {
  file: { icon: File, body: FILE_OUTLINE },
  code: {
    icon: FileCode,
    body: `${FILE_OUTLINE}<path d="M10 12.5 8 15l2 2.5"/><path d="m14 12.5 2 2.5-2 2.5"/>`,
  },
  image: {
    icon: FileImage,
    body: `${FILE_OUTLINE}<circle cx="10" cy="12" r="2"/><path d="m20 17-1.296-1.296a2.41 2.41 0 0 0-3.408 0L9 22"/>`,
  },
  text: {
    icon: FileText,
    body: `${FILE_OUTLINE}<path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/>`,
  },
  type: {
    icon: FileType,
    body: `${FILE_OUTLINE}<path d="M11 18h2"/><path d="M12 12v6"/><path d="M9 13v-.5a.5.5 0 0 1 .5-.5h5a.5.5 0 0 1 .5.5v.5"/>`,
  },
  braces: {
    icon: Braces,
    body: '<path d="M8 3H7a2 2 0 0 0-2 2v5a2 2 0 0 1-2 2 2 2 0 0 1 2 2v5c0 1.1.9 2 2 2h1"/><path d="M16 21h1a2 2 0 0 0 2-2v-5c0-1.1.9-2 2-2a2 2 0 0 1-2-2V5a2 2 0 0 0-2-2h-1"/>',
  },
  database: {
    icon: Database,
    body: '<ellipse cx="12" cy="5" rx="9" ry="3"/><path d="M3 5V19A9 3 0 0 0 21 19V5"/><path d="M3 12A9 3 0 0 0 21 12"/>',
  },
  settings: {
    icon: Settings2,
    body: '<path d="M14 17H5"/><path d="M19 7h-9"/><circle cx="17" cy="17" r="3"/><circle cx="7" cy="7" r="3"/>',
  },
  terminal: {
    icon: TerminalSquare,
    body: '<path d="m7 11 2-2-2-2"/><path d="M11 13h4"/><rect width="18" height="18" x="3" y="3" rx="2" ry="2"/>',
  },
  package: {
    icon: Package,
    body: '<path d="M11 21.73a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73z"/><path d="M12 22V12"/><polyline points="3.29 7 12 12 20.71 7"/><path d="m7.5 4.27 9 5.15"/>',
  },
  folder: {
    icon: Folder,
    body: '<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>',
  },
} satisfies Record<string, Omit<FileIcon, "class">>;

const tint = (glyph: Omit<FileIcon, "class">, colour: string): FileIcon => ({ ...glyph, class: colour });
const code = (colour: string) => tint(glyphs.code, colour);

const byExtension: Record<string, FileIcon> = {
  ts: code("text-blue-500"),
  tsx: code("text-blue-500"),
  mts: code("text-blue-500"),
  cts: code("text-blue-500"),
  js: code("text-yellow-500"),
  jsx: code("text-yellow-500"),
  mjs: code("text-yellow-500"),
  cjs: code("text-yellow-500"),
  svelte: code("text-orange-500"),
  vue: code("text-green-500"),
  rs: code("text-orange-600"),
  py: code("text-blue-400"),
  rb: code("text-red-500"),
  go: code("text-cyan-500"),
  java: code("text-red-400"),
  kt: code("text-purple-500"),
  swift: code("text-orange-500"),
  c: code("text-blue-600"),
  h: code("text-blue-600"),
  cc: code("text-blue-600"),
  cpp: code("text-blue-600"),
  hpp: code("text-blue-600"),
  cs: code("text-violet-500"),
  php: code("text-indigo-400"),
  lua: code("text-blue-500"),
  zig: code("text-orange-500"),
  graphql: code("text-pink-500"),
  html: code("text-orange-500"),
  css: tint(glyphs.type, "text-sky-500"),
  scss: tint(glyphs.type, "text-pink-500"),
  json: tint(glyphs.braces, "text-amber-500"),
  jsonc: tint(glyphs.braces, "text-amber-500"),
  yaml: tint(glyphs.settings, "text-surface-500"),
  yml: tint(glyphs.settings, "text-surface-500"),
  toml: tint(glyphs.settings, "text-surface-500"),
  env: tint(glyphs.settings, "text-surface-500"),
  md: tint(glyphs.text, "text-surface-500"),
  mdx: tint(glyphs.text, "text-surface-500"),
  txt: tint(glyphs.text, "text-surface-500"),
  rst: tint(glyphs.text, "text-surface-500"),
  csv: tint(glyphs.text, "text-surface-500"),
  log: tint(glyphs.text, "text-surface-500"),
  pdf: tint(glyphs.text, "text-red-500"),
  doc: tint(glyphs.text, "text-blue-500"),
  docx: tint(glyphs.text, "text-blue-500"),
  sql: tint(glyphs.database, "text-teal-500"),
  db: tint(glyphs.database, "text-teal-500"),
  sh: tint(glyphs.terminal, "text-green-600"),
  bash: tint(glyphs.terminal, "text-green-600"),
  zsh: tint(glyphs.terminal, "text-green-600"),
  png: tint(glyphs.image, "text-purple-400"),
  jpg: tint(glyphs.image, "text-purple-400"),
  jpeg: tint(glyphs.image, "text-purple-400"),
  gif: tint(glyphs.image, "text-purple-400"),
  svg: tint(glyphs.image, "text-purple-400"),
  webp: tint(glyphs.image, "text-purple-400"),
  ico: tint(glyphs.image, "text-purple-400"),
  bmp: tint(glyphs.image, "text-purple-400"),
  avif: tint(glyphs.image, "text-purple-400"),
  heic: tint(glyphs.image, "text-purple-400"),
  lock: tint(glyphs.package, "text-surface-500"),
};

const genericFile = tint(glyphs.file, "text-surface-500");

/** The icon used for directories, in the picker and on mention chips. */
export const folderIcon = tint(glyphs.folder, "text-surface-500");

export function fileIconFor(fileName: string): FileIcon {
  const extension = fileName.includes(".") ? (fileName.split(".").pop() ?? "").toLowerCase() : "";
  return byExtension[extension] ?? genericFile;
}

/**
 * Icon for a path, treating a trailing slash as "directory" — the convention
 * Codex uses when it writes folder mentions (`[cli](packages/cli/)`).
 */
export function iconForPath(fileName: string, path: string): FileIcon {
  return path.endsWith("/") || fileName.endsWith("/") ? folderIcon : fileIconFor(fileName);
}

/** Renders an icon as standalone SVG markup, for chips built as raw DOM. */
export function fileIconSvg({ body }: FileIcon, className: string): string {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="${className}">${body}</svg>`;
}

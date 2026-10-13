/**
 * Picking a highlight.js grammar for a file path.
 *
 * The `hljs.getLanguage` check at the end is the whole point of this module,
 * not defensive padding: `highlightAs` falls back to `hljs.highlightAuto` for
 * an unknown name, and auto-detection runs *every* registered grammar over the
 * input. On a large file that is a multi-second main-thread freeze, so a name
 * this app cannot highlight must come back as `plaintext` instead.
 */

import hljs from "highlight.js/lib/common";
import { basename } from "$lib/utils/path";

/** Files whose whole name, not extension, names the language. */
const BY_NAME: Record<string, string> = {
  makefile: "makefile",
  gemfile: "ruby",
  rakefile: "ruby",
  procfile: "yaml",
};

/**
 * Extension → grammar. Every value must be a grammar
 * `highlight.js/lib/common` actually registers — a test pins that — so adding
 * one (`dockerfile`, `cmake`, …) means registering it in `markdown.ts` first.
 *
 * `.svelte` is deliberately absent: highlight.js ships no Svelte grammar in any
 * build, and `xml` colours the markup while leaving runes and `{#if}` blocks
 * plain — misleading enough that plaintext is the honest answer.
 */
const BY_EXTENSION: Record<string, string> = {
  ts: "typescript",
  tsx: "typescript",
  mts: "typescript",
  cts: "typescript",
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  rs: "rust",
  py: "python",
  pyi: "python",
  rb: "ruby",
  go: "go",
  java: "java",
  kt: "kotlin",
  kts: "kotlin",
  swift: "swift",
  c: "c",
  h: "c",
  cc: "cpp",
  cpp: "cpp",
  cxx: "cpp",
  hpp: "cpp",
  cs: "csharp",
  php: "php",
  pl: "perl",
  pm: "perl",
  lua: "lua",
  r: "r",
  sql: "sql",
  m: "objectivec",
  mm: "objectivec",
  sh: "bash",
  bash: "bash",
  zsh: "bash",
  fish: "bash",
  css: "css",
  scss: "scss",
  less: "less",
  html: "xml",
  htm: "xml",
  xml: "xml",
  svg: "xml",
  json: "json",
  jsonc: "json",
  json5: "json",
  yaml: "yaml",
  yml: "yaml",
  toml: "ini",
  ini: "ini",
  cfg: "ini",
  md: "markdown",
  mdx: "markdown",
  markdown: "markdown",
  diff: "diff",
  patch: "diff",
  graphql: "graphql",
  gql: "graphql",
  vb: "vbnet",
  wat: "wasm",
};

/** A grammar name highlight.js has registered, or `"plaintext"`. */
export function languageForPath(path: string): string {
  const name = basename(path).toLowerCase();
  // Whole-name matches first: `Dockerfile.dev` has an extension that means
  // nothing, and `CMakeLists.txt` is not a text file.
  const byName = BY_NAME[name];
  if (byName) return registered(byName);

  const dot = name.lastIndexOf(".");
  if (dot <= 0) return "plaintext";
  return registered(BY_EXTENSION[name.slice(dot + 1)]);
}

/** The last gate before `highlightAuto` — see the note at the top. */
function registered(name: string | undefined): string {
  return name && hljs.getLanguage(name) ? name : "plaintext";
}

/** Exported for the test that pins every mapped name to a real grammar. */
export const MAPPED_LANGUAGES = [...Object.values(BY_NAME), ...Object.values(BY_EXTENSION)];

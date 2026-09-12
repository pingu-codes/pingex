import hljs from "highlight.js/lib/common";
import { describe, expect, it } from "vitest";
import { languageForPath, MAPPED_LANGUAGES } from "./codeLanguage";

describe("languageForPath", () => {
  it("maps common source extensions", () => {
    expect(languageForPath("/a/b/review.test.ts")).toBe("typescript");
    expect(languageForPath("/a/b/main.rs")).toBe("rust");
    expect(languageForPath("/a/b/app.py")).toBe("python");
    expect(languageForPath("/a/b/Cargo.toml")).toBe("ini");
    expect(languageForPath("/a/b/README.md")).toBe("markdown");
  });

  it("falls back to plaintext for grammars highlight.js does not ship", () => {
    // Not a gap to fix by reaching for the full bundle; see the module notes.
    expect(languageForPath("/a/b/Thing.svelte")).toBe("plaintext");
    expect(languageForPath("/a/b/main.zig")).toBe("plaintext");
    expect(languageForPath("/a/b/App.vue")).toBe("plaintext");
  });

  it("falls back to plaintext when there is no usable extension", () => {
    expect(languageForPath("/a/b/LICENSE")).toBe("plaintext");
    expect(languageForPath("/a/b/.gitignore")).toBe("plaintext");
    expect(languageForPath("")).toBe("plaintext");
  });

  it("matches whole names before extensions", () => {
    expect(languageForPath("/a/b/Makefile")).toBe("makefile");
    expect(languageForPath("/a/b/Gemfile")).toBe("ruby");
  });

  it("reads the basename, not the directory", () => {
    expect(languageForPath("/a/b.rs/c.ts")).toBe("typescript");
    expect(languageForPath("C:\\a\\b\\main.rs")).toBe("rust");
  });

  /**
   * The guard that matters: an unregistered name would send `highlightAs` into
   * `hljs.highlightAuto`, which runs every grammar over the file.
   */
  it("only ever names a grammar highlight.js has registered", () => {
    for (const name of MAPPED_LANGUAGES) {
      expect(hljs.getLanguage(name), `${name} is not in highlight.js/lib/common`).toBeTruthy();
    }
  });
});

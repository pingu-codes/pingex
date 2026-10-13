import { describe, expect, it } from "vitest";
import { guessKind, isImagePath } from "$lib/utils/media";

describe("isImagePath", () => {
  it("matches image extensions case-insensitively", () => {
    expect(isImagePath("shot.PNG")).toBe(true);
    expect(isImagePath("/tmp/a b/diff.webp")).toBe(true);
    expect(isImagePath("C:\\pics\\photo.heic")).toBe(true);
    expect(isImagePath("icon.avif")).toBe(true);
    expect(isImagePath("favicon.ico")).toBe(true);
  });

  it("rejects non-images and bare names", () => {
    expect(isImagePath("notes.md")).toBe(false);
    expect(isImagePath("/tmp/png")).toBe(false);
    expect(isImagePath("/tmp/.png")).toBe(false);
    expect(isImagePath("/tmp.d/README")).toBe(false);
  });
});

describe("guessKind", () => {
  it("trusts an image MIME type over the extension", () => {
    expect(guessKind("pasted", "image/png")).toBe("image");
  });

  it("falls back to the extension", () => {
    expect(guessKind("photo.jpeg")).toBe("image");
    expect(guessKind("data.csv", "text/csv")).toBe("file");
  });
});

import { render } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";
import CodeView from "./CodeView.svelte";

const numbers = (container: HTMLElement) =>
  (container.querySelector('[data-testid="gutter"]')?.textContent ?? "").split("\n");

describe("CodeView", () => {
  it("numbers every line", () => {
    const { container } = render(CodeView, { text: "a\nb\nc", language: "plaintext" });
    expect(numbers(container)).toEqual(["1", "2", "3"]);
  });

  it("does not add a phantom row for a trailing newline", () => {
    const { container } = render(CodeView, { text: "a\nb\n", language: "plaintext" });
    expect(numbers(container)).toEqual(["1", "2"]);
  });

  it("keeps alignment for a file starting with a blank line", () => {
    const { container } = render(CodeView, { text: "\na\nb", language: "plaintext" });
    expect(numbers(container)).toEqual(["1", "2", "3"]);
    // The `<pre>` leading-newline swallow only bites when the newline is a
    // direct text child of `<pre>`; it is inside `<code>` here.
    expect(container.querySelector("code.hljs")?.textContent?.startsWith("\n")).toBe(true);
  });

  it("numbers a file with no newlines as one line", () => {
    const { container } = render(CodeView, { text: "only", language: "plaintext" });
    expect(numbers(container)).toEqual(["1"]);
  });

  it("draws a band for the cited line", () => {
    const { container } = render(CodeView, { text: "a\nb\nc", language: "plaintext", highlightLine: 2 });
    const band = container.querySelector('[data-testid="line-band"]');
    expect(band?.getAttribute("style")).toContain("1 * var(--code-line)");
  });

  it("draws no band when the cited line is past the end of the file", () => {
    const { container } = render(CodeView, { text: "a\nb", language: "plaintext", highlightLine: 99 });
    expect(container.querySelector('[data-testid="line-band"]')).toBeNull();
  });

  it("highlights normally sized files", () => {
    const { container } = render(CodeView, { text: "const x = 1;\n", language: "typescript" });
    expect(container.querySelector("code.hljs")?.innerHTML).toContain("hljs-");
  });

  it("skips highlighting past the size guard", () => {
    // Wide rather than tall, so this measures the byte guard and not the gutter.
    const text = `const x = "${"y".repeat(300_000)}";\n`;
    const { container } = render(CodeView, { text, language: "typescript" });
    const code = container.querySelector("code.hljs");
    expect(code?.innerHTML).not.toContain("hljs-");
    expect(code?.textContent).toContain("const x =");
  });

  it("escapes markup when the size guard skips highlighting", () => {
    const text = `<script>alert(1)</script>${"y".repeat(300_000)}`;
    const { container } = render(CodeView, { text, language: "plaintext" });
    const code = container.querySelector("code.hljs");
    expect(code?.querySelector("script")).toBeNull();
    expect(code?.textContent).toContain("<script>alert(1)</script>");
  });
});

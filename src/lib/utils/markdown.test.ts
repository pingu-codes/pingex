import { describe, expect, it } from "vitest";
import { escapeHtml, highlightDiff, renderMarkdown } from "$lib/utils/markdown";

describe("renderMarkdown", () => {
  it("wraps gfm tables in a scrollable container", () => {
    const html = renderMarkdown("| Area | Added |\n|---|---|\n| CLI | 1,035 |");
    expect(html).toContain('<div class="table-wrap">');
    expect(html).toContain("<table>");
    expect(html).toContain("<th>Area</th>");
    expect(html).toContain("<td>1,035</td>");
  });

  it("adds a copy button to code blocks that survives sanitization", () => {
    const html = renderMarkdown("```ts\nconst x = 1;\n```");
    expect(html).toContain('<div class="code-block code-dark">');
    expect(html).toContain('class="code-copy"');
    expect(html).toContain('aria-label="Copy code"');
    expect(html).toContain('<code class="hljs">');
  });

  it("shows an unlabeled block plain while it streams, then highlights it once closed", () => {
    const code = 'function greet() { return "<hi>"; }';
    const streaming = renderMarkdown(`Here:\n\n\`\`\`\n${code}`);
    expect(streaming).not.toContain("hljs-");
    expect(streaming).toContain("&lt;hi&gt;");

    const closed = renderMarkdown(`Here:\n\n\`\`\`\n${code}\n\`\`\``);
    expect(closed).toContain("hljs-");
  });

  it("still highlights a labeled block while it streams", () => {
    expect(renderMarkdown("```js\nconst x = 1;")).toContain("hljs-");
  });
});

describe("escapeHtml", () => {
  it("escapes markup characters and leaves the rest alone", () => {
    expect(escapeHtml('<a href="x">&</a>')).toBe('&lt;a href="x"&gt;&amp;&lt;/a&gt;');
    expect(escapeHtml("plain text")).toBe("plain text");
  });
});

describe("highlightDiff", () => {
  const patch = "@@ -1 +1 @@\n-<old>\n+<new>";

  it("highlights a patch within the limit", () => {
    const html = highlightDiff(patch, 1000);
    expect(html).toContain("hljs-");
    expect(html).toContain("&lt;new&gt;");
  });

  it("only escapes a patch over the limit", () => {
    expect(highlightDiff(patch, 10)).toBe(escapeHtml(patch));
  });

  it("defaults to a limit that highlights ordinary diffs", () => {
    expect(highlightDiff(patch)).toContain("hljs-");
  });
});

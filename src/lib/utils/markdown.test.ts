import { describe, expect, it } from "vitest";
import { renderMarkdown } from "$lib/utils/markdown";

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

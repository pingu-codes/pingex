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
});

import { describe, expect, it } from "vitest";

import { cut, renderMarkdown, renderMarkdownBlocks } from "./markdown";

describe("renderMarkdown", () => {
  it("renders task-list items as read-only checkboxes, not text fields", () => {
    const html = renderMarkdown("- [x] done\n- [ ] open\n");
    const box = new DOMParser().parseFromString(html, "text/html").querySelectorAll("input");
    expect(box).toHaveLength(2);
    expect([...box].map((b) => [b.type, b.checked, b.disabled])).toEqual([
      ["checkbox", true, true],
      ["checkbox", false, true],
    ]);
  });

  it("renders GFM and keeps safe links and raster data images", () => {
    const html = renderMarkdown("# T\n\n- [x] done\n\n[ok](https://example.com)\n\n![p](data:image/png;base64,iVBOR)");
    expect(html).toContain("<h1>T</h1>");
    expect(html).toMatch(/<input[^>]*checked[^>]*disabled/);
    expect(html).toContain('href="https://example.com"');
    expect(html).toContain('rel="noopener noreferrer"');
    expect(html).toContain('src="data:image/png;base64,iVBOR"');
  });

  it("syntax-highlights a fenced code block in a known language", () => {
    const html = renderMarkdown("```rust\nfn main() {}\n```\n");
    expect(html).toContain('<pre><code class="hljs language-rust">');
    expect(html).toContain('class="hljs-keyword"');
    expect(html).toContain(">fn<");
  });

  it("falls back to plain escaped text for an unknown or missing fence language", () => {
    const known = renderMarkdown("```made-up-lang\n<tag>\n```\n");
    expect(known).toContain('<pre><code class="hljs">');
    expect(known).toContain("&lt;tag&gt;");
    expect(known).not.toContain("language-made-up-lang");

    const bare = renderMarkdown("```\nplain text\n```\n");
    expect(bare).toContain('<pre><code class="hljs">plain text</code></pre>');
  });

  it("resolves a common language alias to the same highlighting as its canonical name", () => {
    const viaAlias = renderMarkdown("```js\nconst x = 1;\n```\n");
    const canonical = renderMarkdown("```javascript\nconst x = 1;\n```\n");
    expect(viaAlias).toBe(canonical);
    expect(viaAlias).toContain("language-javascript");
  });

  it("strips script, handlers, javascript: and data: hrefs, svg data images, styles", () => {
    const html = renderMarkdown(
      [
        "<script>alert(1)</script>",
        '<img src=x onerror="alert(1)">',
        '<a href="javascript:alert(1)">js</a>',
        "[svg](data:image/svg+xml;base64,PHN2Zz4=)",
        "![svg](data:image/svg+xml;base64,PHN2Zz4=)",
        '<p style="color:red" onclick="x()">styled</p>',
      ].join("\n\n"),
    );
    expect(html).not.toContain("<script");
    expect(html).not.toContain("onerror");
    expect(html).not.toContain("onclick");
    expect(html).not.toContain("javascript:");
    expect(html).not.toContain("svg+xml");
    expect(html).not.toContain("style=");
    expect(html).toContain("styled");
  });
});

describe("renderMarkdownBlocks", () => {
  it("marks every top-level block with its source line", () => {
    const html = renderMarkdownBlocks("# Title\n\nA paragraph\nwrapped.\n\n- one\n- two\n\n```ts\nconst x = 1;\n```\n");
    const lines = [...html.matchAll(/data-line="(\d+)"/g)].map((m) => Number(m[1]));
    expect(lines).toEqual([0, 2, 5, 8]);
    expect(html).toContain("<h1>Title</h1>");
    expect(html).toContain("hljs");
  });

  it("keeps the marks but still strips what the plain renderer strips", () => {
    const html = renderMarkdownBlocks('<img src=x onerror="alert(1)" data-evil="1">\n\n[x](javascript:alert(1))');
    expect(html).not.toContain("onerror");
    expect(html).not.toContain("data-evil");
    expect(html).not.toContain("javascript:");
    expect(renderMarkdown("# a")).not.toContain("data-line");
  });

  it("never lets a note forge a line mark of its own", () => {
    const html = renderMarkdownBlocks('Text with <span data-line="999">forged</span> html.');
    expect([...html.matchAll(/data-line="(\d+)"/g)].map((m) => m[1])).toEqual(["0"]);
  });
});

describe("cut", () => {
  it("returns text at or under the limit unchanged", () => {
    expect(cut("short", 20)).toBe("short");
    expect(cut("12345", 5)).toBe("12345");
  });

  it("breaks at the last space before the limit and adds an ellipsis", () => {
    expect(cut("hello world foo", 9)).toBe("hello wor…");
  });

  it("breaks at the last newline before the limit, same as a space", () => {
    expect(cut("hello\nworld foo bar", 9)).toBe("hello\nwor…");
  });

  it("hard-cuts at the limit when the nearest break is too far back", () => {
    // No space/newline at all within the slice: falls back to a hard cut.
    expect(cut("abcdefghijklmnopqrst", 10)).toBe("abcdefghij…");
  });

  it("hard-cuts when the nearest break is before 60% of the limit", () => {
    // The only space is at index 2, well under 0.6 * 10 = 6, so it is ignored.
    expect(cut("hi 12345678901234", 10)).toBe("hi 1234567…");
  });

  it("trims trailing whitespace left by the break before adding the ellipsis", () => {
    expect(cut("word ".repeat(40).trim(), 20)).not.toContain("  ");
    expect(cut("word ".repeat(40).trim(), 20).endsWith("…")).toBe(true);
  });
});

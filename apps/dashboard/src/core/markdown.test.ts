import { describe, expect, it } from "vitest";

import { excerpt, excerptHtml, renderMarkdown } from "./markdown";

describe("renderMarkdown", () => {
  it("renders GFM and keeps safe links and raster data images", () => {
    const html = renderMarkdown("# T\n\n- [x] done\n\n[ok](https://example.com)\n\n![p](data:image/png;base64,iVBOR)");
    expect(html).toContain("<h1>T</h1>");
    expect(html).toMatch(/<input[^>]*checked[^>]*disabled/);
    expect(html).toContain('href="https://example.com"');
    expect(html).toContain('rel="noopener noreferrer"');
    expect(html).toContain('src="data:image/png;base64,iVBOR"');
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

describe("excerpt", () => {
  it("drops frontmatter and the title, flattens syntax, cuts at a word", () => {
    const e = excerpt("---\ntags: [a]\n---\n# Title\n\nSome **bold** and a [[Wiki Link|alias]] and [x](y.md).\n\n- [ ] task\n- item\n\n```ts\nconst x = 1;\n```\n", 80);
    expect(e.startsWith("Some bold and a Wiki Link and x.")).toBe(true);
    expect(e).toContain("• task");
    expect(e).not.toContain("```");
    expect(e).not.toContain("# Title");
  });
  it("cuts long text at a word boundary with an ellipsis", () => {
    const e = excerpt("word ".repeat(300), 100);
    expect(e.endsWith("…")).toBe(true);
    expect(e.length).toBeLessThanOrEqual(101);
  });
});

describe("excerptHtml", () => {
  it("strips style/script/tags and decodes entities", () => {
    const e = excerptHtml("<html><head><title>T</title><style>body{}</style></head><body><h1>Variablen &amp; Datentypen</h1><p>let &lt;x&gt;</p><script>alert(1)</script></body></html>");
    expect(e).toBe("T\nVariablen & Datentypen\nlet <x>");
    expect(excerptHtml("<title>Same</title><h1>Same</h1><p>body</p>")).toBe("Same\nbody");
  });
});

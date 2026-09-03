import { describe, expect, it } from "vitest";

import { renderMarkdown } from "./markdown";

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

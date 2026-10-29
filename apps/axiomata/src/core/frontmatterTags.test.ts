import { describe, expect, it } from "vitest";

import { frontmatterTags, MAX_CHIPS } from "./frontmatterTags";

describe("frontmatterTags", () => {
  it("reads a block list, a flow list and plain text", () => {
    const block = "---\ntitle: X\ntags:\n  - AI/News\n  - \"#rust\"\n\n  - rust\nother: 1\n---\n# X\n";
    expect(frontmatterTags(block)).toEqual(["ai/news", "rust"]);
    expect(frontmatterTags("---\ntags: [ki-news, 'Ai/Models']\n---\nText\n")).toEqual(["ki-news", "ai/models"]);
    expect(frontmatterTags("---\ntag: a, b c\n---\n")).toEqual(["a", "b", "c"]);
    expect(frontmatterTags("﻿---\r\ntags:\r\n  - x\r\n---\r\nBody")).toEqual(["x"]);
  });

  it("finds nothing without a leading frontmatter or a tags key, and drops what is no tag", () => {
    expect(frontmatterTags("# Title\n\n---\ntags: [a]\n---\n")).toEqual([]);
    expect(frontmatterTags("---\ntitle: X\n---\n#inline\n")).toEqual([]);
    expect(frontmatterTags("---\ntags:\n  - 123\n  - a b\n  - <img src=x>\n  - ok\n---\n")).toEqual(["ok"]);
    // The closing fence is no list item, and a key after the list ends it.
    expect(frontmatterTags("---\ntags:\n- one\n---\n- two\n")).toEqual(["one"]);
  });

  it("stops at the cap", () => {
    const many = Array.from({ length: MAX_CHIPS + 50 }, (_, i) => `t${i}`).join(", ");
    expect(frontmatterTags(`---\ntags: [${many}]\n---\n`)).toHaveLength(MAX_CHIPS);
  });
});

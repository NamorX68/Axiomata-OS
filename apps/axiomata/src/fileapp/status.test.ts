import { describe, expect, it } from "vitest";

import { EditorDocument } from "../editor/document";
import { cursor, pos } from "../editor/position";
import { statusParts } from "./status";

const FALLBACK = { indentFallback: { kind: "spaces" as const, size: 4 } };

describe("statusParts", () => {
  it("counts the column with tabs expanded and wide characters", () => {
    const doc = new EditorDocument("\tx😀y", FALLBACK);
    doc.setSelection(cursor(pos(0, 4)));
    expect(statusParts(doc, 4).position).toBe("Ln 1, Col 8");
  });

  it("names the line ending and the indentation, and where it came from", () => {
    expect(statusParts(new EditorDocument("a\r\nb", FALLBACK), 4)).toMatchObject({
      eol: "CRLF",
      indent: "4 spaces (default)",
    });
    expect(statusParts(new EditorDocument("a\r\nb\nc", FALLBACK), 4).eol).toBe("LF (was mixed)");
    expect(statusParts(new EditorDocument("a\n  b\n    c", FALLBACK), 4).indent).toBe("2 spaces");
    expect(statusParts(new EditorDocument("a\n\tb", FALLBACK), 4).indent).toBe("Tabs");
  });
});

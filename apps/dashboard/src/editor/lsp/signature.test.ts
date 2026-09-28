import { describe, expect, it } from "vitest";

import { parseSignatureHelp } from "./signature";

describe("parseSignatureHelp (ED6.6)", () => {
  it("marks the active parameter by offsets or by its text, and prefers the signature's own index", () => {
    const byOffsets = parseSignatureHelp({
      signatures: [
        {
          label: "fn push(&mut self, value: T)",
          parameters: [{ label: [8, 17] }, { label: [19, 27], documentation: "The **value**." }],
        },
      ],
      activeParameter: 1,
    });
    expect(byOffsets).toEqual({
      before: "fn push(&mut self, ",
      active: "value: T",
      after: ")",
      documentation: "The **value**.",
      overloads: null,
    });
    const byText = parseSignatureHelp({
      signatures: [
        { label: "f(a, b)", parameters: [{ label: "a" }, { label: "b" }], activeParameter: 0 },
        { label: "f(a)", parameters: [{ label: "a" }] },
      ],
      activeSignature: 0,
      activeParameter: 1,
      // The first `a` after `(` — not the one in a name before it.
    });
    expect(byText).toMatchObject({ before: "f(", active: "a", after: ", b)", overloads: "1/2" });
  });

  it("shows a signature without a known parameter unmarked, and nothing for an empty answer", () => {
    expect(parseSignatureHelp({ signatures: [{ label: "g()", documentation: { kind: "plaintext", value: "a*b" } }] }))
      .toEqual({ before: "g()", active: "", after: "", documentation: "a\\*b", overloads: null });
    expect(parseSignatureHelp({ signatures: [] })).toBeNull();
    expect(parseSignatureHelp(null)).toBeNull();
  });
});

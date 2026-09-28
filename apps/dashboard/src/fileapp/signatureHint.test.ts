import { describe, expect, it } from "vitest";

import { run, UNWRAPPED } from "../editor/commands";
import { EditorDocument } from "../editor/document";
import type { SignatureView } from "../editor/lsp/signature";
import { cursor, pos, type Pos } from "../editor/position";
import { SignatureHint, type SignaturePort } from "./signatureHint";

const ctx = { tabSize: 4, layout: UNWRAPPED, pageRows: 10, commentPrefix: null };
const sig = (active: string): SignatureView => ({ before: "f(", active, after: ")", documentation: null, overloads: null });

async function settle(): Promise<void> {
  for (let i = 0; i < 10; i++) await Promise.resolve();
}

function setup(text: string, at: Pos) {
  const asked: { trigger: string | null; retrigger: boolean; answer: (s: SignatureView | null) => void }[] = [];
  const port: SignaturePort = {
    ask: (_at, trigger, retrigger) => new Promise((answer) => asked.push({ trigger, retrigger, answer })),
    triggers: () => ["(", ","],
    retriggers: () => [")"],
  };
  const doc = new EditorDocument(text, { indentFallback: { kind: "spaces", size: 4 } });
  doc.setSelection(cursor(at));
  const hint = new SignatureHint(port, () => {});
  const type = (t: string) => {
    run(doc, { type: "insert", text: t }, ctx);
    hint.typed(doc, t);
  };
  return { asked, doc, hint, type };
}

describe("SignatureHint (ED6.6)", () => {
  it("opens on a trigger character, refreshes as the arguments are typed, and closes when the call is left", async () => {
    const { asked, hint, type } = setup("f", pos(0, 1));
    type("x");
    expect(asked).toHaveLength(0);
    type("(");
    expect(asked[0]).toMatchObject({ trigger: "(", retrigger: false });
    asked[0].answer(sig("a"));
    await settle();
    expect(hint.view?.signature.active).toBe("a");
    type("1");
    expect(asked[1]).toMatchObject({ trigger: null, retrigger: true });
    type(",");
    expect(asked[2]).toMatchObject({ trigger: ",", retrigger: true });
    asked[1].answer(sig("stale"));
    asked[2].answer(sig("b"));
    await settle();
    expect(hint.view?.signature.active).toBe("b");
    type(")");
    expect(asked[3]).toMatchObject({ trigger: ")", retrigger: true });
    asked[3].answer(null);
    await settle();
    expect(hint.isOpen).toBe(false);
  });

  it("closes when the cursor leaves the line, and drops an answer that comes after a close", async () => {
    const { asked, doc, hint, type } = setup("f\nnext", pos(0, 1));
    type("(");
    asked[0].answer(sig("a"));
    await settle();
    doc.setSelection(cursor(pos(1, 0)));
    hint.follow(doc);
    expect(hint.isOpen).toBe(false);
    hint.invoke(doc);
    hint.close();
    asked[1].answer(sig("late"));
    await settle();
    expect(hint.isOpen).toBe(false);
  });
});

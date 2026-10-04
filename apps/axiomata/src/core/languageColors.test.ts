import { describe, expect, it } from "vitest";

import { languageColor, languageColorId } from "./languageColors";

describe("language colours (K14)", () => {
  it("names the language of a file, TSX as TypeScript, anything unknown as other", () => {
    expect(languageColorId("src/main.rs")).toBe("rust");
    expect(languageColorId("App.tsx")).toBe("typescript");
    expect(languageColorId("notes/Idee.md")).toBe("markdown");
    expect(languageColorId("run", "#!/usr/bin/env bash")).toBe("bash");
    expect(languageColorId("photo.png")).toBe("other");
    expect(languageColor("a.py")).toBe("var(--ax-lang-python)");
  });
});

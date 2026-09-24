import { describe, expect, it } from "vitest";

import { detectLanguage, languageForName, LANGUAGES } from "./languages";
import { tokenFor } from "./tokens";

describe("detectLanguage (G11)", () => {
  it("goes by whole name, then extension", () => {
    expect(detectLanguage("src/main.rs")).toBe("rust");
    expect(detectLanguage("App.svelte")).toBe("svelte");
    expect(detectLanguage("View.TSX")).toBe("tsx");
    expect(detectLanguage("Cargo.lock")).toBe("toml");
    expect(detectLanguage("home/.zshrc")).toBe("bash");
    expect(detectLanguage("notes.txt")).toBeNull();
  });

  it("reads a shebang when there is no extension", () => {
    expect(detectLanguage("deploy", "#!/usr/bin/env python3")).toBe("python");
    expect(detectLanguage("run", "#!/bin/zsh -e")).toBe("bash");
    expect(detectLanguage("tool", "#!/usr/bin/env ruby")).toBeNull();
    expect(detectLanguage("README")).toBeNull();
  });
});

describe("languageForName", () => {
  it("resolves fence names and aliases to bundled languages only", () => {
    expect(languageForName("rs")).toBe("rust");
    expect(languageForName(" TS ")).toBe("typescript");
    expect(languageForName("sh")).toBe("bash");
    expect(languageForName("haskell")).toBeNull();
  });

  it("covers the fifteen owner-chosen languages (G2)", () => {
    const ids = new Set(LANGUAGES.map((l) => l.id));
    for (const id of ["rust", "typescript", "javascript", "svelte", "markdown", "json", "toml", "yaml"]) {
      expect(ids.has(id)).toBe(true);
    }
    for (const id of ["css", "html", "python", "bash", "swift", "lua", "sql"]) expect(ids.has(id)).toBe(true);
  });
});

describe("tokenFor (G5)", () => {
  it("falls back from a specific capture to its family", () => {
    expect(tokenFor("function.method.call")).toBe("function");
    expect(tokenFor("keyword.return")).toBe("keyword");
    expect(tokenFor("punctuation.bracket")).toBe("punctuation");
    expect(tokenFor("string.special.key")).toBe("property");
  });

  it("maps older and Markdown vocabularies", () => {
    expect(tokenFor("conditional")).toBe("keyword");
    expect(tokenFor("field")).toBe("property");
    expect(tokenFor("text.title")).toBe("heading");
    expect(tokenFor("text.uri")).toBe("link");
    expect(tokenFor("text.literal")).toBe("code");
    expect(tokenFor("markup.italic")).toBe("emphasis");
  });

  it("does not mistake object-prototype names for table entries", () => {
    expect(tokenFor("constructor")).toBe("type");
    expect(tokenFor("toString")).toBeNull();
    expect(tokenFor("hasOwnProperty.x")).toBeNull();
  });

  it("leaves uncoloured names and unknown ones alone", () => {
    expect(tokenFor("none")).toBeNull();
    expect(tokenFor("spell")).toBeNull();
    expect(tokenFor("embedded")).toBeNull();
    expect(tokenFor("frobnicate.special")).toBeNull();
  });
});

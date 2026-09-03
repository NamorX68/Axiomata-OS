import { describe, expect, it } from "vitest";

import { assetFileUrl } from "./backend";

describe("assetFileUrl", () => {
  it("keeps '/' as literal path separators instead of encoding the whole path", () => {
    expect(assetFileUrl("/Users/roman/Documents/vault/Learning/Rust/lessons/0002-variablen.html")).toBe(
      "asset://localhost/Users/roman/Documents/vault/Learning/Rust/lessons/0002-variablen.html",
    );
  });

  it("still percent-encodes special characters within a segment, but not the slash itself", () => {
    expect(assetFileUrl("/Users/roman/a b/c#d.html")).toBe("asset://localhost/Users/roman/a%20b/c%23d.html");
  });

  it("resolves a relative link the same way a browser would, staying in the same folder", () => {
    const base = assetFileUrl("/vault/Learning/Rust/lessons/0002-variablen.html");
    const resolved = new URL("0003-funktionen.html", base).href;
    expect(resolved).toBe("asset://localhost/vault/Learning/Rust/lessons/0003-funktionen.html");
  });
});

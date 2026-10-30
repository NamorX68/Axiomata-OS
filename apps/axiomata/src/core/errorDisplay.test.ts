import { describe, expect, it } from "vitest";

/**
 * A backend error may arrive coded (`␟code␟{json}␟English`, i18n.md Q11): only `messageOf` (`core/errors.ts`) turns it
 * into the owner's sentence. `String(err)` would show the raw code, so no source file may use it on a caught error.
 */
const SOURCES = import.meta.glob("../**/*.{ts,svelte}", { query: "?raw", import: "default", eager: true }) as Record<
  string,
  string
>;

/** `String(` of a caught error under one of the usual names. */
const RAW_ERROR = /\bString\((err|e|error)\)/g;

/** Justified uses, by file (relative to `src/`) and the exact line text, each with why it stays. */
const ALLOWED: { file: string; line: string; why: string }[] = [
  {
    file: "modules/index.ts",
    line: "if (/no such file|not found|does not exist/i.test(String(err))) {",
    why: "matches the English wording of a missing file, never shown — a translated sentence would not match",
  },
];

/** A glob key as a path under `src/`: vite names this folder's own files `./x`, all others `../dir/x`. */
function sourceFile(key: string): string {
  return key.startsWith("./") ? `core/${key.slice(2)}` : key.replace(/^\.\.\//, "");
}

/** Files that are not app code reaching the owner: the converter itself, tests, the app-free editor engine. */
function exempt(file: string): boolean {
  return file === "core/errors.ts" || /\.test\.ts$/.test(file) || file.startsWith("editor/");
}

describe("backend errors are shown through messageOf", () => {
  it("has sources to check", () => {
    expect(Object.keys(SOURCES).length).toBeGreaterThan(50);
  });

  it("no source turns a caught error into text with String(err)", () => {
    const offenders: string[] = [];
    for (const [path, text] of Object.entries(SOURCES)) {
      const file = sourceFile(path);
      if (exempt(file)) continue;
      text.split("\n").forEach((raw, index) => {
        if (!RAW_ERROR.test(raw)) return;
        RAW_ERROR.lastIndex = 0;
        const line = raw.trim();
        if (ALLOWED.some((a) => a.file === file && a.line === line)) return;
        offenders.push(`${file}:${index + 1}: ${line}`);
      });
    }
    expect(offenders).toEqual([]);
  });

  it("every allow-list entry still exists", () => {
    for (const allowed of ALLOWED) {
      const text = Object.entries(SOURCES).find(([path]) => sourceFile(path) === allowed.file)?.[1];
      expect(text, allowed.file).toBeDefined();
      expect((text ?? "").split("\n").map((l) => l.trim()), allowed.why).toContain(allowed.line);
    }
  });
});

/**
 * Every catalog of the app, both languages side by side (`docs/plans/i18n.md`, Q10, Q20, Q23): the same values named
 * in both, no German text left empty, and none of the forms the owner never wants to read.
 */

import { describe, expect, it } from "vitest";

import { catalogs, type Text } from "./texts";

// Loads every catalog module of the folder, so a new area is checked without being listed here.
import.meta.glob(["./*.ts", "!./*.test.ts", "!./texts.ts"], { eager: true });

const names = (text: string): string[] =>
  [...text.matchAll(/\{([a-zA-Z][a-zA-Z0-9_]*)\}/g)].map((m) => m[1]).sort();
const forms = (text: Text): string[] =>
  typeof text === "string" ? [text] : [text.one, text.other];

/** Gendered forms (Q23): a star, colon, underscore or capital I before "innen", and the participle stand-ins. */
const GENDERED =
  /[*:_]innen\b|[a-zäöü]Innen\b|\b(Benutzende|Nutzende|Teilnehmende|Mitarbeitende)\b/;

describe("the catalogs", () => {
  const all = catalogs().filter((c) => !c.area.startsWith("test-"));

  it("are loaded", () => {
    expect(all.map((c) => c.area)).toContain("format");
    expect(all.map((c) => c.area)).toContain("settings");
  });

  it("name the same values in German as in English, and leave no German text empty", () => {
    for (const { area, en, de } of all) {
      for (const key of Object.keys(en)) {
        const [e, d] = [en[key], de[key]];
        expect(d, `${area}.${key}`).toBeDefined();
        if (e === undefined || d === undefined) continue;
        const english = forms(e);
        const german = forms(d);
        german.forEach((form, i) => {
          expect(form.trim(), `${area}.${key}`).not.toBe("");
          expect(names(form), `${area}.${key}`).toEqual(
            names(english[i] ?? ""),
          );
        });
      }
    }
  });

  it("never gender (Q23)", () => {
    for (const { area, de } of all) {
      for (const [key, text] of Object.entries(de)) {
        for (const form of forms(text))
          expect(form, `${area}.${key}`).not.toMatch(GENDERED);
      }
    }
  });
});

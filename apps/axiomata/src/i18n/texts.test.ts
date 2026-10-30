import { afterEach, describe, expect, it } from "vitest";

import {
  lang,
  languageSetting,
  locale,
  resolveLang,
  systemLang,
} from "./lang.svelte";
import { fill, textIn, texts } from "./texts";

afterEach(() => languageSetting.set("system"));

describe("the language", () => {
  it("is the first of the system's languages the app speaks, else English", () => {
    expect(systemLang(["fr-FR", "de-AT", "en-US"])).toBe("de");
    expect(systemLang(["en_GB", "de-DE"])).toBe("en");
    expect(systemLang(["fr-FR", "ja"])).toBe("en");
    expect(systemLang([])).toBe("en");
  });

  it("follows the owner's choice over the system's", () => {
    expect(resolveLang("de", ["en-US"])).toBe("de");
    expect(resolveLang("en", ["de-DE"])).toBe("en");
    expect(resolveLang("system", ["de-DE"])).toBe("de");
  });

  it("switches at once, and dates and numbers follow it", () => {
    languageSetting.set("de");
    expect(lang()).toBe("de");
    expect(locale()).toBe("de-DE");
    expect(document.documentElement.lang).toBe("de");
    languageSetting.set("en");
    expect(lang()).toBe("en");
    expect(locale()).toBe("en-GB");
  });
});

describe("a text", () => {
  const en = {
    hello: "Hello {name}",
    notes: { one: "1 note", other: "{n} notes" },
  };
  const de = {
    hello: "Hallo {name}",
    notes: { one: "1 Notiz", other: "{n} Notizen" },
  };

  it("fills the values it names and keeps a name without a value visible", () => {
    expect(fill("{a} and {b}", { a: 1 })).toBe("1 and {b}");
    expect(fill("{n} items", { n: 0 })).toBe("0 items");
  });

  it("takes the form for one only for exactly one", () => {
    expect(textIn(en, de, "de", "notes", { n: 1 })).toBe("1 Notiz");
    expect(textIn(en, de, "de", "notes", { n: 0 })).toBe("0 Notizen");
    expect(textIn(en, de, "en", "notes", { n: 3 })).toBe("3 notes");
  });

  it("is looked up in the language of the moment", () => {
    const t = texts("test-switch", en, de);
    languageSetting.set("de");
    expect(t("hello", { name: "du" })).toBe("Hallo du");
    languageSetting.set("en");
    expect(t("hello", { name: "you" })).toBe("Hello you");
  });
});

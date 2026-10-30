import { afterEach, describe, expect, it } from "vitest";

import { languageSetting } from "../i18n/lang.svelte";
import { englishOf, messageOf } from "./errors";

afterEach(() => languageSetting.set("system"));

/** A coded refusal as a Tauri command sends it (`AxiomataError::for_ui`). */
const coded = (code: string, params: Record<string, string>, english: string) =>
  `\u001f${code}\u001f${JSON.stringify(params)}\u001f${english}`;

describe("messageOf", () => {
  it("returns a string error as-is", () => {
    expect(messageOf("plain string failure")).toBe("plain string failure");
  });

  it("reads an Error's own message, including a subclass", () => {
    expect(messageOf(new Error("bad thing happened"))).toBe(
      "bad thing happened",
    );
    expect(messageOf(new TypeError("wrong type"))).toBe("wrong type");
  });

  it("reads the message field off a file-service-shaped error ({ kind, message })", () => {
    expect(messageOf({ kind: "NotFound", message: "no such file" })).toBe(
      "no such file",
    );
  });

  it("falls back to String() for an object with no message field", () => {
    expect(messageOf({ kind: "NotFound" })).toBe("[object Object]");
  });

  it("falls back to String() for null, undefined and primitives", () => {
    expect(messageOf(null)).toBe("null");
    expect(messageOf(undefined)).toBe("undefined");
    expect(messageOf(42)).toBe("42");
  });

  it("writes a coded refusal in the app's language, and an unknown code as Rust's English sentence", () => {
    languageSetting.set("de");
    expect(
      messageOf(coded("role.unknown", { name: "x" }, "there is no role x")),
    ).toBe("Es gibt keine Rolle „x“ für dieses Projekt.");
    expect(
      messageOf(new Error(coded("card.missing", { id: "7" }, "no card 7"))),
    ).toBe("Es gibt keine Karte 7.");
    expect(
      messageOf(coded("no.such.code", {}, "There is no such thing.")),
    ).toBe("There is no such thing.");
    expect(messageOf(coded("role.unknown", {}, "Broken\u001fparts"))).toBe(
      "Es gibt keine Rolle „{name}“ für dieses Projekt.",
    );
    expect(
      messageOf("\u001frole.unknown\u001fnot json\u001fThe English."),
    ).toBe("The English.");
    languageSetting.set("en");
    expect(messageOf(coded("card.missing", { id: "7" }, "no card 7"))).toBe(
      "No card 7.",
    );
  });

  it("gives a model the English sentence of a coded refusal, never the code nor a translation", () => {
    languageSetting.set("de");
    const err = coded("role.unknown", { name: "x" }, "There is no role “x”.");
    expect(englishOf(err)).toBe("There is no role “x”.");
    expect(englishOf(new Error(err))).toBe("There is no role “x”.");
    expect(englishOf("plain failure")).toBe("plain failure");
  });
});

/**
 * The parts of the app already in two languages show no text of their own (`docs/plans/i18n.md`, Q5): every word a
 * converted component draws comes from a catalog. A step of the plan adds its files to `CONVERTED`; a text written
 * straight into one of them fails here instead of staying English unnoticed.
 */

import { describe, expect, it } from "vitest";

/** Every component's source, read on demand: only the converted ones are loaded. */
const SOURCES = import.meta.glob<string>("../**/*.svelte", { query: "?raw", import: "default" });

/** The converted components, relative to `src/`. */
const CONVERTED = ["shell/LanguageSetting.svelte"];

/** Marks and names that may stand as they are: names stay English in both languages (Q13). */
const NAMES = "Studio|Canvas|Flow|Kanban|Orbit|Skills?|Routines?|Master";
const ALLOWED = new RegExp(`^(?:[\\s\\d·•…—–\\-+×/:,.()[\\]{}<>|?!%#@&*'"]|${NAMES})*$`);

/** The markup of a component without its script, its style and its comments. */
function markup(source: string): string {
  return source
    .replace(/<script[\s\S]*?<\/script>/g, "")
    .replace(/<style[\s\S]*?<\/style>/g, "")
    .replace(/<!--[\s\S]*?-->/g, "");
}

/** The `{expressions}` of a piece of markup, outermost first (braces counted, so `{() => {…}}` is one). */
function expressions(html: string): { inner: string; start: number; end: number }[] {
  const found: { inner: string; start: number; end: number }[] = [];
  let depth = 0;
  let start = -1;
  for (let i = 0; i < html.length; i++) {
    if (html[i] === "{") {
      if (depth === 0) start = i;
      depth++;
    } else if (html[i] === "}" && depth > 0) {
      depth--;
      if (depth === 0) found.push({ inner: html.slice(start + 1, i), start, end: i + 1 });
    }
  }
  return found;
}

/** A string literal of an expression that reads as words: not a key, a class, a path or an option name. */
const WORDS = /(["'`])((?:(?!\1)[^\\]|\\.)*[A-Za-z]{2,}(?:(?!\1)[^\\]|\\.)*\s(?:(?!\1)[^\\]|\\.)*)\1/g;

/**
 * Literal texts the markup shows: text between tags, the attributes a person reads (also around an `{expression}`),
 * and a string inside an expression that reads as words (`{c ? "Foo bar" : x}`). Keys and options (`t("startAgent")`,
 * `{ dateStyle: "short" }`) have no space and pass.
 */
export function literalTexts(source: string): string[] {
  const html = markup(source);
  const found: string[] = [];
  const keep = (text: string) => {
    const trimmed = text.trim();
    if (trimmed && !ALLOWED.test(trimmed)) found.push(trimmed);
  };
  const exprs = expressions(html);
  for (const e of exprs) for (const m of e.inner.matchAll(WORDS)) keep(m[2] ?? "");
  // The markup with every expression blanked, so `=>` or `>` inside one does not read as a tag's end.
  let plain = "";
  let at = 0;
  for (const e of exprs) {
    plain += html.slice(at, e.start) + " ".repeat(e.end - e.start);
    at = e.end;
  }
  plain += html.slice(at);
  for (const m of plain.matchAll(/>([^<>]+)</g)) keep(m[1] ?? "");
  for (const m of plain.matchAll(/\b(?:label|title|aria-label|placeholder|alt)="([^"]*)"/g)) keep(m[1] ?? "");
  return found;
}

describe("the converted components", () => {
  it("show no text of their own", async () => {
    for (const file of CONVERTED) {
      const load = SOURCES[`../${file}`];
      expect(load, `${file} exists`).toBeDefined();
      if (!load) continue;
      expect(literalTexts(await load()), file).toEqual([]);
    }
  });

  it("would catch a text written straight into the markup", () => {
    expect(literalTexts('<p>Hello</p><button title="Close">{t("x")}</button>')).toEqual(["Hello", "Close"]);
    expect(literalTexts("<p>{t('x')} · Studio</p><script>const a = '<b>no</b>';</script>")).toEqual([]);
    expect(literalTexts('<p>{busy ? "Starting now" : t("start")}</p>')).toEqual(["Starting now"]);
    expect(literalTexts('<button title="Open {name}" onclick={() => go(a > b)}>{t("go")}</button>')).toEqual(["Open"]);
    expect(literalTexts('<p>{formatDate(x, { dateStyle: "short" })}</p>')).toEqual([]);
  });
});

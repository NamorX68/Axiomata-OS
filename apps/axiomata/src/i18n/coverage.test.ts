/**
 * The parts of the app already in two languages show no text of their own (`docs/plans/i18n.md`, Q5): every word a
 * component draws comes from a catalog — since I7 every component of the app; one that must show text of its own is
 * named in `EXEMPT` with the reason. A text written straight into the markup fails here instead of staying English.
 */

import { describe, expect, it } from "vitest";

/** Every component's source, read on demand. */
const SOURCES = import.meta.glob<string>("../**/*.svelte", {
  query: "?raw",
  import: "default",
});

/**
 * Every component is converted (I1–I7): none draws text of its own. A component that must, names why here.
 */
const EXEMPT: readonly string[] = [];

/** Marks and names that may stand as they are: names stay English in both languages (Q13). */
const NAMES =
  "Studio|Canvas|Flow|Kanban|Orbit|Skills?|Routines?|Master|Axiomata|Glut|[A-Za-z_]+\\.md|url\\(#\\s*\\)|~/[\\w./-]*";
const MARKS = `[\\s\\d·•…—–\\-+×/:,.()[\\]{}<>|?!%#@&*'"›‹→←↑↓−●✎✕✓↻↩★☆⌘⇧⌥⌃⏎]`;
const ALLOWED = new RegExp(`^(?:${MARKS}|${NAMES})*$`);

/** The markup of a component without its script, its style and its comments. */
function markup(source: string): string {
  return source
    .replace(/<script[\s\S]*?<\/script>/g, "")
    .replace(/<style[\s\S]*?<\/style>/g, "")
    .replace(/<!--[\s\S]*?-->/g, "");
}

/** The `{expressions}` of a piece of markup, outermost first (braces counted, so `{() => {…}}` is one). */
function expressions(
  html: string,
): { inner: string; start: number; end: number }[] {
  const found: { inner: string; start: number; end: number }[] = [];
  let depth = 0;
  let start = -1;
  for (let i = 0; i < html.length; i++) {
    if (html[i] === "{") {
      if (depth === 0) start = i;
      depth++;
    } else if (html[i] === "}" && depth > 0) {
      depth--;
      if (depth === 0)
        found.push({ inner: html.slice(start + 1, i), start, end: i + 1 });
    }
  }
  return found;
}

/** The string literals of an expression, read quote by quote (so the code between two strings is never one). */
function strings(expression: string): string[] {
  const found: string[] = [];
  let quote: string | null = null;
  let text = "";
  for (let i = 0; i < expression.length; i++) {
    const c = expression[i] ?? "";
    if (quote === null) {
      if (c === '"' || c === "'" || c === "`") {
        quote = c;
        text = "";
      }
    } else if (c === "\\") {
      text += expression[i + 1] ?? "";
      i++;
    } else if (c === quote) {
      found.push(text);
      quote = null;
    } else {
      text += c;
    }
  }
  return found;
}

/** A string that reads as words — two letters and a space — rather than a key, a class, a path or an option. */
const READS_AS_WORDS = /[A-Za-z]{2,}.*\s|\s.*[A-Za-z]{2,}/;

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
  for (const e of exprs) {
    for (const raw of strings(e.inner)) {
      // A template literal's `${…}` parts are code, not words.
      const text = raw.replace(/\$\{[^}]*\}/g, " ");
      if (READS_AS_WORDS.test(text.trim())) keep(text);
    }
  }
  // The markup with every expression blanked, so `=>` or `>` inside one does not read as a tag's end.
  let plain = "";
  let at = 0;
  for (const e of exprs) {
    plain += html.slice(at, e.start) + " ".repeat(e.end - e.start);
    at = e.end;
  }
  plain += html.slice(at);
  for (const m of plain.matchAll(/>([^<>]+)</g)) keep(m[1] ?? "");
  for (const m of plain.matchAll(
    /\b(?:label|title|aria-label|placeholder|alt)="([^"]*)"/g,
  ))
    keep(m[1] ?? "");
  return found;
}

describe("the converted components", () => {
  it("show no text of their own", async () => {
    const files = Object.keys(SOURCES).filter(
      (path) => !EXEMPT.includes(path.slice(3)),
    );
    expect(files.length).toBeGreaterThan(100);
    const found: Record<string, string[]> = {};
    for (const path of files) {
      const load = SOURCES[path];
      if (!load) continue;
      const texts = literalTexts(await load());
      if (texts.length > 0) found[path.slice(3)] = texts;
    }
    expect(found).toEqual({});
  });

  it("would catch a text written straight into the markup", () => {
    expect(
      literalTexts('<p>Hello</p><button title="Close">{t("x")}</button>'),
    ).toEqual(["Hello", "Close"]);
    expect(
      literalTexts(
        "<p>{t('x')} · Studio</p><script>const a = '<b>no</b>';</script>",
      ),
    ).toEqual([]);
    expect(literalTexts('<p>{busy ? "Starting now" : t("start")}</p>')).toEqual(
      ["Starting now"],
    );
    expect(
      literalTexts(
        '<button title="Open {name}" onclick={() => go(a > b)}>{t("go")}</button>',
      ),
    ).toEqual(["Open"]);
    expect(
      literalTexts('<p>{formatDate(x, { dateStyle: "short" })}</p>'),
    ).toEqual([]);
    expect(
      literalTexts('<b>{t("newSession", { keys: hint("⌘N", p) })}</b>'),
    ).toEqual([]);
  });
});

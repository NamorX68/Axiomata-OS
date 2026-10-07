import { describe, expect, it } from "vitest";

import {
  GROUP_LABEL,
  KIND_ORDER,
  MAX_PER_KIND,
  MAX_ROWS,
  normalize,
  rankItems,
  scoreMatch,
  type SpotlightItem,
  type SpotlightKind,
} from "./spotlight";

const item = (kind: SpotlightKind, id: string, title: string, rest: Partial<SpotlightItem> = {}): SpotlightItem => ({
  kind,
  id,
  title,
  ...rest,
});

const titles = (rows: ReturnType<typeof rankItems>): string[] => rows.map((row) => row.item.title);

describe("normalize", () => {
  it("lower-cases and strips accents", () => {
    expect(normalize("Pläne Café")).toBe("plane cafe");
  });
});

describe("scoreMatch", () => {
  it("ranks exact above prefix above word start above substring above scattered letters", () => {
    const exact = scoreMatch("mail", "mail")!;
    const prefix = scoreMatch("mail", "mail digest")!;
    const wordStart = scoreMatch("mail", "daily mail")!;
    const substring = scoreMatch("mail", "gmail")!;
    const scattered = scoreMatch("mdg", "mail digest")!;
    expect([exact, prefix, wordStart, substring, scattered]).toEqual(
      [...[exact, prefix, wordStart, substring, scattered]].sort((a, b) => b - a),
    );
    expect(new Set([exact, prefix, wordStart, substring, scattered]).size).toBe(5);
  });

  it("is case- and accent-insensitive", () => {
    expect(scoreMatch("PLANE", "Pläne")).toBe(scoreMatch("plane", "plane"));
    expect(scoreMatch("cafe", "Café")).not.toBeNull();
  });

  it("needs every word of a longer query, in any order", () => {
    expect(scoreMatch("digest mail", "Mail digest")).not.toBeNull();
    expect(scoreMatch("mail kanban", "Mail digest")).toBeNull();
  });

  it("returns null for no match and for an empty query", () => {
    expect(scoreMatch("zzz", "Mail digest")).toBeNull();
    expect(scoreMatch("   ", "Mail digest")).toBeNull();
  });

  it("matches scattered letters only from three letters on", () => {
    expect(scoreMatch("md", "mail digest")).toBeNull();
    expect(scoreMatch("mdg", "mail digest")).not.toBeNull();
  });

  it("prefers a hit nearer the start of the text", () => {
    expect(scoreMatch("note", "a note")!).toBeGreaterThan(scoreMatch("note", "a long long long long long note")!);
  });
});

describe("rankItems", () => {
  it("shows the suggestions, as given, for an empty query", () => {
    const suggestions = [item("command", "new-note", "Neue Notiz"), item("command", "settings", "Einstellungen")];
    const rows = rankItems("  ", { suggestions, skill: [item("skill", "s", "Mail")] });
    expect(titles(rows)).toEqual(["Neue Notiz", "Einstellungen"]);
    expect(rows.map((row) => row.group)).toEqual(["suggestion", "suggestion"]);
    expect(rows.map((row) => row.groupStart)).toEqual([true, false]);
  });

  it("returns nothing when nothing matches", () => {
    expect(rankItems("zzzz", { skill: [item("skill", "s", "Mail digest")] })).toEqual([]);
  });

  it("puts a prefix hit before a substring hit of the same kind", () => {
    const rows = rankItems("mail", {
      skill: [item("skill", "a", "gmail cleanup"), item("skill", "b", "mail digest")],
    });
    expect(titles(rows)).toEqual(["mail digest", "gmail cleanup"]);
  });

  it("ranks a name hit above a hit in the subtitle, and above one only the content has", () => {
    const rows = rankItems("budget", {
      file: [
        item("file", "content", "notes.md", { contentMatches: 40 }),
        item("file", "sub", "plan.md", { subtitle: "Budget 2027" }),
        item("file", "name", "budget.md"),
      ],
    });
    expect(rows.map((row) => row.item.id)).toEqual(["name", "sub", "content"]);
  });

  it("keeps a content-only file below even the weakest name hit", () => {
    const rows = rankItems("bdg", {
      file: [item("file", "content", "notes.md", { contentMatches: 50 }), item("file", "scattered", "b-u-d-g.md")],
    });
    expect(rows.map((row) => row.item.id)).toEqual(["scattered", "content"]);
  });

  it("orders the groups by kind and marks the first row of each", () => {
    const rows = rankItems("a", {
      file: [item("file", "f", "a.md")],
      card: [item("card", "c", "a card")],
      command: [item("command", "m", "add")],
    });
    expect(rows.map((row) => row.group)).toEqual(["command", "card", "file"]);
    expect(rows.every((row) => row.groupStart)).toBe(true);
    const two = rankItems("a", { skill: [item("skill", "1", "a1"), item("skill", "2", "a2")] });
    expect(two.map((row) => row.groupStart)).toEqual([true, false]);
  });

  it("caps one kind so that a long list of cards does not push the other groups out", () => {
    const cards = Array.from({ length: 30 }, (_, i) => item("card", `c${i}`, `kanban card ${i}`));
    const rows = rankItems("kanban", { card: cards, module: [item("module", "k", "Kanban")] });
    expect(rows.filter((row) => row.group === "card")).toHaveLength(MAX_PER_KIND);
    expect(rows.some((row) => row.group === "module")).toBe(true);
  });

  it("caps the whole list, keeping the best rows whatever their kind", () => {
    const sources = Object.fromEntries(
      KIND_ORDER.map((kind) => [kind, Array.from({ length: 6 }, (_, i) => item(kind, `${kind}${i}`, `item ${i}`))]),
    );
    const rows = rankItems("item", sources);
    expect(rows).toHaveLength(MAX_ROWS);
    const positions = rows.map((row) => KIND_ORDER.indexOf(row.group as SpotlightKind));
    expect(positions).toEqual([...positions].sort((a, b) => a - b));
  });

  it("breaks ties by the shorter title, then alphabetically, so the order is stable", () => {
    const rows = rankItems("mail", {
      skill: [item("skill", "b", "mail zeta"), item("skill", "a", "mail beta"), item("skill", "c", "mail"),
        item("skill", "d", "mail alpha")],
    });
    expect(titles(rows)).toEqual(["mail", "mail beta", "mail zeta", "mail alpha"]);
  });

  it("matches the keywords too, but below the title", () => {
    const rows = rankItems("cron", {
      routine: [item("routine", "k", "Nightly", { keywords: "cron 0 0 * * *" }), item("routine", "t", "cron check")],
    });
    expect(rows.map((row) => row.item.id)).toEqual(["t", "k"]);
  });

  it("labels every group the list can show", () => {
    for (const kind of KIND_ORDER) expect(GROUP_LABEL[kind]).toBeTruthy();
    expect(GROUP_LABEL.suggestion).toBeTruthy();
  });
});

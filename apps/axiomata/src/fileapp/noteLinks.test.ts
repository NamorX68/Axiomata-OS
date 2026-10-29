import { describe, expect, it } from "vitest";

import {
  codeLines,
  codeSpans,
  fileStem,
  headingLine,
  lineMarks,
  markAt,
  newNotePath,
  normaliseRel,
  NoteIndex,
  parseWikilink,
  type NoteFile,
} from "./noteLinks";

const lines = (text: string) => {
  const all = text.split("\n");
  return { lineCount: all.length, line: (n: number) => all[n] };
};

const note = (path: string, tags?: string[]): NoteFile => ({
  path,
  area: path.includes("/") ? path.split("/")[0] : null,
  is_markdown: path.endsWith(".md"),
  tags,
});

describe("parseWikilink", () => {
  it("splits target, heading and alias", () => {
    expect(parseWikilink("Note")).toEqual({ target: "Note", heading: null, alias: null });
    expect(parseWikilink("Note|shown")).toEqual({ target: "Note", heading: null, alias: "shown" });
    expect(parseWikilink("Note#Part two|x")).toEqual({ target: "Note", heading: "Part two", alias: "x" });
    expect(parseWikilink("#Only here")).toEqual({ target: "", heading: "Only here", alias: null });
  });
});

describe("lineMarks", () => {
  it("finds wikilinks and tags left to right", () => {
    const marks = lineMarks("See [[Rust/Ownership|ownership]] and #rust/async, #Ki-News.");
    expect(marks.map((m) => (m.kind === "link" ? `link:${m.target}` : `tag:${m.tag}`))).toEqual([
      "link:Rust/Ownership",
      "tag:rust/async",
      "tag:ki-news",
    ]);
    expect(marks[0]).toMatchObject({ from: 4, to: 32, alias: "ownership" });
  });

  it("takes umlauts, digits, _ - and / in a tag", () => {
    expect(lineMarks("#Übersicht_2026-q1/ä")).toMatchObject([{ kind: "tag", tag: "übersicht_2026-q1/ä" }]);
  });

  it("leaves out headings, numbers, mid-word #s and URLs", () => {
    expect(lineMarks("# Title")).toEqual([]);
    expect(lineMarks("## Title #real")).toMatchObject([{ kind: "tag", tag: "real" }]);
    expect(lineMarks("Issue #12 and #3")).toEqual([]);
    expect(lineMarks("C# and a#b")).toEqual([]);
    expect(lineMarks("https://example.com/page#frag and [x](#anchor)")).toEqual([]);
  });

  it("leaves out what is in inline code", () => {
    expect(lineMarks("`[[Not]] #not` but [[Yes]] #yes")).toMatchObject([
      { kind: "link", target: "Yes" },
      { kind: "tag", tag: "yes" },
    ]);
    expect(lineMarks("``a ` [[Not]]`` [[Yes]]")).toMatchObject([{ kind: "link", target: "Yes" }]);
  });

  it("finds no tag inside a link's heading part", () => {
    expect(lineMarks("[[Note #part]]")).toMatchObject([{ kind: "link", target: "Note" }]);
  });

  it("knows the mark under a column", () => {
    expect(markAt("go [[A]] now", 4)).toMatchObject({ kind: "link", target: "A" });
    expect(markAt("go [[A]] now", 8)).toBeNull();
  });
});

describe("codeSpans and codeLines", () => {
  it("pairs backtick runs of the same length", () => {
    expect(codeSpans("a `b` c ``d`e`` f `g")).toEqual([
      [2, 5],
      [8, 15],
    ]);
  });

  it("marks fenced blocks and the frontmatter", () => {
    const text = "---\ntags: [a]\n---\ntext\n```ts\n#x [[y]]\n```\nafter\n~~~~\n```\n~~~~\nend";
    expect([...codeLines(lines(text))].sort((a, b) => a - b)).toEqual([0, 1, 2, 4, 5, 6, 8, 9, 10]);
  });

  it("leaves an unclosed frontmatter as text", () => {
    expect(codeLines(lines("---\ntext #tag"))).toEqual(new Set());
  });
});

describe("NoteIndex.resolve — the Rust scan's rule", () => {
  const index = new NoteIndex([
    note("KI/Modelle.md"),
    note("KI/Sub/Deep.md"),
    note("Arbeit/Plan.md"),
    note("Persönlich/Plan.md"),
    note("README.md"),
    note("Learning/page.html"),
  ]);

  it("takes a path relative to the note's folder first, then from the root", () => {
    expect(index.resolve("KI/Start.md", "Sub/Deep")).toBe("KI/Sub/Deep.md");
    expect(index.resolve("KI/Start.md", "../Arbeit/Plan")).toBe("Arbeit/Plan.md");
    expect(index.resolve("Other/x.md", "KI/Modelle")).toBe("KI/Modelle.md");
    expect(index.resolve("Other/x.md", "Learning/page.html")).toBe("Learning/page.html");
  });

  it("falls back to a unique stem, ignoring case", () => {
    expect(index.resolve("Other/x.md", "modelle")).toBe("KI/Modelle.md");
    expect(index.resolve("Other/x.md", "readme")).toBe("README.md");
  });

  it("finds nothing for an ambiguous stem or a missing note", () => {
    expect(index.resolve("Other/x.md", "Plan")).toBeNull();
    expect(index.resolve("Other/x.md", "Nowhere")).toBeNull();
    expect(index.resolve("Other/x.md", "  ")).toBeNull();
    expect(index.sameStem("plan")).toEqual(["Arbeit/Plan.md", "Persönlich/Plan.md"]);
  });

  it("writes the shortest link that finds a note", () => {
    expect(index.linkText("KI/Modelle.md")).toBe("Modelle");
    expect(index.linkText("Arbeit/Plan.md")).toBe("Arbeit/Plan");
  });

  it("gathers the vault's tags", () => {
    const tagged = new NoteIndex([note("a.md", ["rust", "KI"]), note("b.md", ["ki"]), note("c.md")]);
    expect(tagged.tags()).toEqual(["ki", "rust"]);
  });
});

describe("helpers", () => {
  it("fileStem and normaliseRel follow the Rust scan", () => {
    expect(fileStem("A/B/note.md")).toBe("note");
    expect(fileStem("noext")).toBe("noext");
    expect(normaliseRel("a/b", "../c.md")).toBe("a/c.md");
    expect(normaliseRel("a", "/c.md")).toBe("c.md");
    expect(normaliseRel("", "../../c.md")).toBe("c.md");
  });

  it("headingLine finds a heading by its text", () => {
    const text = "# Top\n\ntext\n## Part Two ##\nmore";
    expect(headingLine(text, "part two")).toBe(3);
    expect(headingLine(text, "top")).toBe(0);
    expect(headingLine(text, "missing")).toBeNull();
  });

  it("newNotePath puts a new note beside the one linking to it", () => {
    expect(newNotePath("KI/Start.md", "Neue Idee")).toBe("KI/Neue Idee.md");
    expect(newNotePath("Start.md", "Folder/Idee.md")).toBe("Idee.md");
    expect(newNotePath("KI/Start.md", "..")).toBeNull();
    expect(newNotePath("KI/Start.md", ".hidden")).toBeNull();
  });
});

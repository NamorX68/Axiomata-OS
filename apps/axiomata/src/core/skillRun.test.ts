import { describe, expect, it } from "vitest";

import type { RunSummary } from "./backend";
import { firstJsonObject, staleDigestNote } from "./skillRun";

describe("staleDigestNote", () => {
  const now = Date.parse("2026-09-29T12:00:00Z");
  const run = (started_at: string) => ({ started_at }) as RunSummary;

  it("is empty when no run was passed over", () => {
    expect(staleDigestNote({ run: run("2026-09-29T09:00:00Z"), skipped: null }, now)).toBe("");
  });

  it("says which run was unusable and how old the shown digest is", () => {
    const note = staleDigestNote(
      { run: run("2026-09-29T09:00:00Z"), skipped: { run: run("2026-09-29T11:00:00Z"), reason: "it produced no output" } },
      now,
    );
    expect(note).toBe("The latest run (1 h ago) could not be used: it produced no output — showing the digest from 3 h ago.");
  });
});

describe("firstJsonObject", () => {
  it("finds the object behind leading prose", () => {
    expect(firstJsonObject('Now I have the data.\n{"emails": []}')).toBe('{"emails": []}');
  });

  it("stops at the first object and never reaches a trailing duplicate", () => {
    expect(firstJsonObject('{"a": 1}{"a": 2')).toBe('{"a": 1}');
  });

  it("skips a balanced {…} in the prose that is not JSON", () => {
    const reply = 'The format is {emails: [...]}, here it is:\n{"emails": [{"id": "1"}]}';
    expect(firstJsonObject(reply)).toBe('{"emails": [{"id": "1"}]}');
  });

  it("ignores braces inside strings", () => {
    expect(firstJsonObject('{"summary": "a } and a {"}')).toBe('{"summary": "a } and a {"}');
  });

  it("is null for a truncated object and for text without braces", () => {
    expect(firstJsonObject('{"emails": [{"id": "1"}')).toBeNull();
    expect(firstJsonObject("no json here")).toBeNull();
  });

  it("hands back a lone balanced-but-invalid object, so callers keep their own parse error", () => {
    expect(firstJsonObject('{"a": 1,}')).toBe('{"a": 1,}');
  });
});

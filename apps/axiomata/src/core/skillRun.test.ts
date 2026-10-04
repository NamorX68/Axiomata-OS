import { describe, expect, it } from "vitest";

import { firstJsonObject } from "./skillRun";

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

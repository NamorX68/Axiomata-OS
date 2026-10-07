import { describe, expect, it } from "vitest";

import { queryTextOrNull } from "./grammarFetch";

describe("queryTextOrNull", () => {
  it("keeps a query that starts with a comment or a pattern", () => {
    expect(queryTextOrNull("; keywords\n(identifier) @variable")).toBe("; keywords\n(identifier) @variable");
    expect(queryTextOrNull('["if" "else"] @keyword')).toBe('["if" "else"] @keyword');
  });

  it("treats the app's index page as no query, whatever the case or leading space", () => {
    expect(queryTextOrNull("<!doctype html><html></html>")).toBeNull();
    expect(queryTextOrNull("\n  <!DOCTYPE html>\n<html>")).toBeNull();
    expect(queryTextOrNull("<html lang=en>")).toBeNull();
  });

  it("keeps an empty body as it is, for the caller to skip", () => {
    expect(queryTextOrNull("")).toBe("");
  });
});

import { describe, expect, it } from "vitest";

import { TEMPLATE, applyCustomCss, validateCustomCss, CUSTOM_STYLE_ID } from "./validator";

function errorsOf(css: string): string[] {
  const r = validateCustomCss(css);
  return r.ok ? [] : r.errors.map((e) => `${e.rule}|${e.property}|${e.message}`);
}

describe("validateCustomCss", () => {
  it("accepts the shipped template", () => {
    expect(validateCustomCss(TEMPLATE).ok).toBe(true);
  });

  it("re-emits accepted declarations on a high-specificity :root selector", () => {
    const r = validateCustomCss(':root { --ax-accent: #00c8ff }\n[data-theme="custom"] { --ax-bg: #000 }');
    expect(r.ok).toBe(true);
    if (!r.ok) return;
    expect(r.css).toContain(":root[data-theme][data-theme] {");
    expect(r.css).toContain("--ax-accent: #00c8ff;");
    expect(r.css).toContain("--ax-bg: #000;");
    expect(r.css).not.toContain('[data-theme="custom"]');
  });

  it("rejects forbidden text before parsing", () => {
    expect(errorsOf("@import url(x.css); :root { --ax-accent: red }")[0]).toMatch(/@import/);
    expect(errorsOf(":root { --ax-texture-url: url(javascript:alert(1)) }")[0]).toMatch(/javascript/);
    expect(errorsOf(":root { --ax-accent: expression(1) }")[0]).toMatch(/expression/);
  });

  it("rejects other selectors, at-rules, non-token and unknown properties", () => {
    const errs = errorsOf(
      "body { color: red }\n:root { --ax-accent: red; color: blue; --ax-nope: 1 }\n@media (x) { :root { --ax-bg: red } }",
    );
    expect(errs).toEqual([
      expect.stringMatching(/^body\|\|selector must be :root/),
      expect.stringMatching(/^:root\|color\|only --ax-\* tokens/),
      expect.stringMatching(/^:root\|--ax-nope\|not an overridable token/),
      expect.stringMatching(/only plain style rules/),
    ]);
  });

  it("allows url() only for data:image URIs", () => {
    expect(errorsOf(':root { --ax-texture-url: url("https://x/y.png") }')[0]).toMatch(/data:image/);
    expect(validateCustomCss(':root { --ax-texture-url: url("data:image/png;base64,iVBOR") }').ok).toBe(true);
  });
});

describe("applyCustomCss", () => {
  it("injects, replaces and removes the style element", () => {
    applyCustomCss(":root { --ax-accent: red }");
    const el = document.getElementById(CUSTOM_STYLE_ID);
    expect(el?.textContent).toContain("red");
    applyCustomCss(":root { --ax-accent: blue }");
    expect(document.querySelectorAll(`#${CUSTOM_STYLE_ID}`)).toHaveLength(1);
    expect(document.getElementById(CUSTOM_STYLE_ID)?.textContent).toContain("blue");
    applyCustomCss(null);
    expect(document.getElementById(CUSTOM_STYLE_ID)).toBeNull();
  });
});

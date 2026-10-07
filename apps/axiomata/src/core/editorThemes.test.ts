import { describe, expect, it } from "vitest";

import mainSource from "../main.ts?raw";
import { SYNTAX_TOKENS } from "../editor/syntax/tokens";
import { THEMES as TERMINAL_THEMES } from "../modules/terminalThemes";
import { applyEditorTheme, EDITOR_THEMES, editorThemeOr, FOLLOW_APP_THEME } from "./editorThemes";
import { THEMES } from "./themes";

/** Every stylesheet of the themes directory, as text, by file name. */
const sheets = import.meta.glob<string>("../themes/*.css", { query: "?raw", import: "default", eager: true });
const read = (file: string): string => sheets[`../themes/${file}`] ?? "";

/** The declarations of the block opened by `selector`. */
function block(css: string, selector: string): string {
  const start = css.indexOf(`${selector} {`);
  expect(start, `${selector} exists`).toBeGreaterThanOrEqual(0);
  return css.slice(start, css.indexOf("\n}", start));
}

describe("editor themes", () => {
  const css = read("editor-themes.css");

  it("every listed theme has a block that sets all syntax colours, the surface and the text", () => {
    for (const theme of EDITOR_THEMES) {
      const body = block(css, `[data-editor-theme="${theme.id}"]`);
      for (const token of SYNTAX_TOKENS) expect(body, `${theme.id} ${token}`).toContain(`--ax-syntax-${token}:`);
      for (const name of ["editor-bg", "editor-fg", "editor-current-line", "editor-indent-guide", "editor-bracket-1"]) {
        expect(body, `${theme.id} ${name}`).toContain(`--ax-${name}:`);
      }
    }
  });

  it("has no block for an id that is not listed", () => {
    const ids = [...css.matchAll(/\[data-editor-theme="([^"]+)"\]/g)].map((m) => m[1]);
    expect(ids.sort()).toEqual(EDITOR_THEMES.map((t) => t.id).sort());
  });

  it("every editor theme is also a terminal theme", () => {
    for (const theme of EDITOR_THEMES) expect(Object.keys(TERMINAL_THEMES), theme.id).toContain(theme.id);
  });

  it("applies a theme to <html>, and clears it for follow or an unknown id", () => {
    const root = document.createElement("html");
    applyEditorTheme("nord", root);
    expect(root.dataset.editorTheme).toBe("nord");
    applyEditorTheme(FOLLOW_APP_THEME, root);
    expect(root.dataset.editorTheme).toBeUndefined();
    applyEditorTheme("nord", root);
    applyEditorTheme("no-such-theme", root);
    expect(root.dataset.editorTheme).toBeUndefined();
  });

  it("reads a saved value defensively", () => {
    expect(editorThemeOr("dracula")).toBe("dracula");
    expect(editorThemeOr("nope")).toBe(FOLLOW_APP_THEME);
    expect(editorThemeOr(42)).toBe(FOLLOW_APP_THEME);
  });
});

describe("app themes", () => {
  it("every listed theme has a stylesheet that is imported at start", () => {
    for (const theme of THEMES) {
      const body = block(read(`${theme.id}.css`), `[data-theme="${theme.id}"]`);
      expect(body, theme.id).toContain("--ax-accent:");
      expect(mainSource, theme.id).toContain(`./themes/${theme.id}.css`);
    }
  });
});

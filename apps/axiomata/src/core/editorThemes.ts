/**
 * The editor's own colour themes: a second token layer over the app theme (`themes/editor-themes.css`). The setting
 * `editorTheme` is `"follow"` (the editor wears the app theme's editor colours, as before) or the id of one of
 * `EDITOR_THEMES`; `applyEditorTheme` puts it on `<html data-editor-theme>`, where the CSS block of that id overrides the
 * editor's tokens only.
 */

export interface EditorThemeInfo {
  id: string;
  label: string;
  /** Light themes are marked so the picker can say so. */
  light: boolean;
}

/** Every id has a `[data-editor-theme="…"]` block in `themes/editor-themes.css` (checked by `editorThemes.test.ts`). */
export const EDITOR_THEMES: readonly EditorThemeInfo[] = [
  { id: "catppuccin-mocha", label: "Catppuccin Mocha", light: false },
  { id: "catppuccin-macchiato", label: "Catppuccin Macchiato", light: false },
  { id: "catppuccin-frappe", label: "Catppuccin Frappé", light: false },
  { id: "catppuccin-latte", label: "Catppuccin Latte", light: true },
  { id: "catppuccin-espresso", label: "Catppuccin Espresso", light: false },
  { id: "tokyo-night", label: "Tokyo Night", light: false },
  { id: "github-dark", label: "GitHub Dark", light: false },
  { id: "github-light", label: "GitHub Light", light: true },
  { id: "rose-pine", label: "Rosé Pine", light: false },
  { id: "gruvbox-dark", label: "Gruvbox Dark", light: false },
  { id: "nord", label: "Nord", light: false },
  { id: "dracula", label: "Dracula", light: false },
  { id: "one-dark", label: "One Dark", light: false },
];

/** The setting's value for "use the app theme's editor colours". */
export const FOLLOW_APP_THEME = "follow";

/** `value` if it names an editor theme, else "follow" — a hand-edited or older settings file never breaks the editor. */
export function editorThemeOr(value: unknown): string {
  return typeof value === "string" && EDITOR_THEMES.some((t) => t.id === value) ? value : FOLLOW_APP_THEME;
}

/** Sets or clears `<html data-editor-theme>`. */
export function applyEditorTheme(id: string, root: HTMLElement = document.documentElement): void {
  const theme = editorThemeOr(id);
  if (theme === FOLLOW_APP_THEME) delete root.dataset.editorTheme;
  else root.dataset.editorTheme = theme;
}

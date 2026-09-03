/**
 * Validates a user's `~/.axiomata/theme.css` before it is injected.
 *
 * Accepted: plain style rules whose selector is `:root` (or
 * `[data-theme="custom"]` / `:root[data-theme="custom"]`) and whose every
 * declaration is one of the allow-listed `--ax-*` palette tokens. Rejected:
 * any at-rule (`@import`, `@font-face`, …), any other selector, any
 * non-`--ax-` property, `url(` that isn't a `data:` URI, and anything that
 * smells like script. The CSS is parsed by the webview's own engine in a
 * detached document, so what we inspect is what would apply.
 */

export interface CssError {
  rule: string;
  property: string;
  message: string;
}

export type ValidationResult = { ok: true; css: string } | { ok: false; errors: CssError[] };

/** Tokens a custom theme may override — the palette half of tokens.css. */
export const ALLOWED_TOKENS: readonly string[] = [
  "--ax-color-scheme",
  "--ax-bg",
  "--ax-surface-1",
  "--ax-surface-2",
  "--ax-surface-3",
  "--ax-overlay",
  "--ax-text",
  "--ax-text-muted",
  "--ax-text-invert",
  "--ax-accent",
  "--ax-accent-hover",
  "--ax-accent-muted",
  "--ax-success",
  "--ax-warning",
  "--ax-danger",
  "--ax-border",
  "--ax-border-strong",
  "--ax-focus-ring",
  "--ax-font-display",
  "--ax-font-sans",
  "--ax-font-mono",
  "--ax-shadow-tile",
  "--ax-shadow-pop",
  "--ax-shadow-drag",
  "--ax-texture-url",
  "--ax-tracking-wide",
];

const OUTPUT_SELECTOR = ":root[data-theme][data-theme]";
const ALLOWED_SELECTORS = new Set([":root", '[data-theme="custom"]', ':root[data-theme="custom"]']);
const FORBIDDEN_TEXT = [/@import/i, /javascript:/i, /expression\s*\(/i, /<\s*script/i, /-moz-binding/i, /behavior\s*:/i];
const URL_RE = /url\(\s*(['"]?)([^'")]*)\1\s*\)/gi;

export const CUSTOM_STYLE_ID = "ax-custom-theme";

export function validateCustomCss(text: string): ValidationResult {
  const errors: CssError[] = [];
  for (const re of FORBIDDEN_TEXT) {
    if (re.test(text)) errors.push({ rule: "(file)", property: "", message: `forbidden content: ${re.source}` });
  }
  if (errors.length > 0) return { ok: false, errors };

  const doc = document.implementation.createHTMLDocument("theme-check");
  const style = doc.createElement("style");
  style.textContent = text;
  doc.head.appendChild(style);
  const sheet = style.sheet;
  if (!sheet) return { ok: false, errors: [{ rule: "(file)", property: "", message: "could not parse the stylesheet" }] };

  const out: string[] = [];
  for (const rule of Array.from(sheet.cssRules)) {
    // A detached document has no `defaultView`, so no realm-specific
    // `instanceof`; `CSSRule.STYLE_RULE` (1) is the plain-rule discriminator.
    if (rule.type !== CSSRule.STYLE_RULE || !("selectorText" in rule)) {
      errors.push({ rule: rule.cssText.slice(0, 60), property: "", message: "only plain style rules are allowed (no at-rules)" });
      continue;
    }
    const selector = (rule as CSSStyleRule).selectorText.trim();
    if (!ALLOWED_SELECTORS.has(selector)) {
      errors.push({ rule: selector, property: "", message: 'selector must be :root or [data-theme="custom"]' });
      continue;
    }
    const style = (rule as CSSStyleRule).style;
    const decls: string[] = [];
    for (let i = 0; i < style.length; i++) {
      const prop = style.item(i);
      const value = style.getPropertyValue(prop).trim();
      if (!prop.startsWith("--ax-")) {
        errors.push({ rule: selector, property: prop, message: "only --ax-* tokens may be set" });
        continue;
      }
      if (!ALLOWED_TOKENS.includes(prop)) {
        errors.push({ rule: selector, property: prop, message: "not an overridable token" });
        continue;
      }
      let badUrl = false;
      for (const m of value.matchAll(URL_RE)) {
        if (!/^data:image\//i.test(m[2].trim())) badUrl = true;
      }
      if (badUrl) {
        errors.push({ rule: selector, property: prop, message: "url() must be a data:image/… URI" });
        continue;
      }
      decls.push(`  ${prop}: ${value};`);
    }
    // Built-in themes live on `:root[data-theme="…"]`; emit with a higher
    // specificity so the override wins regardless of stylesheet order.
    if (decls.length > 0) out.push(`${OUTPUT_SELECTOR} {\n${decls.join("\n")}\n}`);
  }
  if (errors.length > 0) return { ok: false, errors };
  return { ok: true, css: out.join("\n\n") + "\n" };
}

/** Injects (or replaces) the validated CSS after the built-in themes. */
export function applyCustomCss(css: string | null): void {
  let el = document.getElementById(CUSTOM_STYLE_ID);
  if (!css) {
    el?.remove();
    return;
  }
  if (!el) {
    el = document.createElement("style");
    el.id = CUSTOM_STYLE_ID;
    document.head.appendChild(el);
  }
  el.textContent = css;
}

/** An annotated starter the user can paste into ~/.axiomata/theme.css. */
export const TEMPLATE = `/* Axiomata-OS custom theme — ~/.axiomata/theme.css
 * Only \`:root { --ax-…: … }\` overrides are accepted. Every token below is
 * optional; delete what you don't change. Applied on top of the built-in
 * theme selected in Settings. Reload from Settings → "Reload custom CSS". */
:root {
  /* light | dark — native controls and scrollbars */
  --ax-color-scheme: dark;

  /* surfaces */
  --ax-bg: #0b0b0d;
  --ax-surface-1: #121216;
  --ax-surface-2: #1a1a1f;
  --ax-surface-3: #23232a;
  --ax-overlay: rgba(0, 0, 0, 0.66);

  /* text */
  --ax-text: #f5f5f7;
  --ax-text-muted: #94949c;
  --ax-text-invert: #0b0b0d;

  /* accent + status */
  --ax-accent: #ff7a1a;
  --ax-accent-hover: #ff9448;
  --ax-accent-muted: rgba(255, 122, 26, 0.15);
  --ax-success: #4fd67f;
  --ax-warning: #e6b45f;
  --ax-danger: #f26d6d;

  /* borders + focus */
  --ax-border: #2a2a30;
  --ax-border-strong: #3b3b43;
  --ax-focus-ring: rgba(255, 122, 26, 0.55);

  /* type: display face for the logo line and panel headings */
  --ax-font-display: var(--ax-font-sans);

  /* shadows + background texture (data: URIs only) */
  --ax-shadow-tile: 0 1px 2px rgba(0, 0, 0, 0.45), 0 8px 22px rgba(0, 0, 0, 0.38);
  --ax-texture-url: none;
}
`;

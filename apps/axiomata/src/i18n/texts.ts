/**
 * The app's texts in German and English (`docs/plans/i18n.md`, Q10): one catalog per area, English the source, German
 * of exactly the same shape — `npm run check` fails when a German text is missing, as Glut's catalogs fail its build.
 * A text names its values (`{name}`); a text that counts has a form for one and one for more (`{ one, other }`, chosen
 * by `n`). Names of places, features, roles and tiers stay English in both (Q13); German says "du" and never genders
 * (Q20, Q23).
 *
 * `t(key)` reads the current language, so a template that shows it follows a switch at once (`lang.svelte.ts`).
 */

import { lang, type Lang } from "./lang.svelte";

/** A text that counts: one form for one, one for every other number. */
export interface Plural {
  readonly one: string;
  readonly other: string;
}

export type Text = string | Plural;

/** The German catalog's shape: every English key, a plain text where English has one, a plural where it has one. */
export type Shape<E extends Record<string, Text>> = { readonly [K in keyof E]: E[K] extends string ? string : Plural };

/** The values a text names. */
export type Params = Readonly<Record<string, string | number>>;

/** A catalog's lookup: the text of `key` in the current language, its `{names}` filled from `params`. */
export type Translate<K extends string> = (key: K, params?: Params) => string;

interface Registered {
  readonly area: string;
  readonly en: Readonly<Record<string, Text>>;
  readonly de: Readonly<Record<string, Text>>;
}

const registry: Registered[] = [];

/** Every catalog made so far — for the test that compares their `{names}` across the languages. */
export function catalogs(): readonly Registered[] {
  return registry;
}

/** Fills `{name}` from `params`; a name without a value stays as it is, so a missing value shows, never vanishes. */
export function fill(template: string, params: Params = {}): string {
  return template.replace(/\{([a-zA-Z][a-zA-Z0-9_]*)\}/g, (whole, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : whole,
  );
}

/** The form of `text` for `params.n`: German and English both use one form for exactly 1 and another for the rest. */
function form(text: Text, params: Params | undefined): string {
  if (typeof text === "string") return text;
  return params?.n === 1 ? text.one : text.other;
}

/** The text of `key` in `language` (for a test or a caller that needs one language whatever the setting). */
export function textIn<E extends Record<string, Text>>(
  en: E,
  de: Shape<E>,
  language: Lang,
  key: keyof E & string,
  params?: Params,
): string {
  const text: Text = language === "de" ? de[key] : en[key];
  return fill(form(text, params), params);
}

/** Makes an area's catalog and its lookup. */
export function texts<E extends Record<string, Text>>(area: string, en: E, de: Shape<E>): Translate<keyof E & string> {
  registry.push({ area, en, de });
  return (key, params) => textIn(en, de, lang(), key, params);
}

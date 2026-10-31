/**
 * Which language the app speaks (`docs/plans/i18n.md`, Q2, Q3, Q14): German or English, by default the one macOS
 * prefers, overridden in Settings → Appearance. The setting is a store (persisted in `dashboard.json` by
 * `core/persist.ts`, like the typeface); the language itself is rune state, so every text and every date a template
 * or a `$derived` reads follows a switch at once, without a restart.
 */

import { writable, type Writable } from "svelte/store";

/** The languages the app has texts for; English is the source and the fallback (Q3). */
export type Lang = "en" | "de";

/** What the owner chose: the system's language, or one of ours. */
export type LanguageSetting = "system" | Lang;

export const LANGUAGE_SETTINGS: readonly LanguageSetting[] = ["system", "de", "en"];

/** Whether a stored value names a setting. */
export function isLanguageSetting(value: unknown): value is LanguageSetting {
  return typeof value === "string" && (LANGUAGE_SETTINGS as readonly string[]).includes(value);
}

/**
 * The first of the system's preferred languages the app speaks, else English — the same order Glut follows
 * (`glut::lang::detect`). `preferred` is `navigator.languages` in the webview, which macOS fills from its own list.
 */
export function systemLang(preferred: readonly string[] = globalThis.navigator?.languages ?? []): Lang {
  for (const tag of preferred) {
    const base = tag.toLowerCase().split(/[-_]/)[0];
    if (base === "de" || base === "en") return base;
  }
  return "en";
}

/** The language a setting means on this machine. */
export function resolveLang(setting: LanguageSetting, preferred?: readonly string[]): Lang {
  return setting === "system" ? systemLang(preferred) : setting;
}

/** The setting, persisted by `core/persist.ts`. */
export const languageSetting: Writable<LanguageSetting> = writable("system");

const state = $state<{ lang: Lang }>({ lang: resolveLang("system") });

languageSetting.subscribe((setting) => {
  state.lang = resolveLang(setting);
  if (typeof document !== "undefined") document.documentElement.lang = state.lang;
});

/** The language the app speaks now — reactive where a template or a `$derived` reads it. */
export function lang(): Lang {
  return state.lang;
}

/**
 * The locale dates and numbers are written in (Q12). British English for English: a 24-hour clock and day before
 * month, as the owner reads dates; German as German.
 */
export function locale(): string {
  return state.lang === "de" ? "de-DE" : "en-GB";
}

/**
 * Loads `~/.axiomata/theme.css` (or `settings.customCssPath`) through Rust,
 * validates it, injects it, and exposes the outcome for the Settings dialog.
 */

import { messageOf } from "./errors";
import { writable } from "svelte/store";

import { invokeBackend } from "./backend";
import { getCustomCssPath } from "./persist";
import { applyCustomCss, validateCustomCss, type CssError } from "../theme/validator";
import { tSettings } from "../i18n/settings";

export type CustomThemeStatus = "absent" | "applied" | "invalid" | "error";

export interface CustomThemeState {
  status: CustomThemeStatus;
  errors: CssError[];
  /** The outcome in the current language; empty before the first load. */
  message: () => string;
}

export const customTheme = writable<CustomThemeState>({ status: "absent", errors: [], message: () => "" });

export async function loadCustomTheme(): Promise<CustomThemeState> {
  let state: CustomThemeState;
  try {
    const text = await invokeBackend<string | null>("load_custom_css", { path: getCustomCssPath() });
    if (text === null) {
      applyCustomCss(null);
      state = { status: "absent", errors: [], message: () => tSettings("customAbsent") };
    } else {
      const result = validateCustomCss(text);
      if (result.ok) {
        applyCustomCss(result.css);
        state = { status: "applied", errors: [], message: () => tSettings("customApplied") };
      } else {
        applyCustomCss(null);
        state = { status: "invalid", errors: result.errors, message: () => tSettings("customRejected") };
      }
    }
  } catch (err) {
    applyCustomCss(null);
    const reason = messageOf(err);
    state = { status: "error", errors: [], message: () => reason };
  }
  customTheme.set(state);
  return state;
}

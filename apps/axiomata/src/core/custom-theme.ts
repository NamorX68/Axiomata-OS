/**
 * Loads `~/.axiomata/theme.css` (or `settings.customCssPath`) through Rust,
 * validates it, injects it, and exposes the outcome for the Settings dialog.
 */

import { writable } from "svelte/store";

import { invokeBackend } from "./backend";
import { getCustomCssPath } from "./persist";
import { applyCustomCss, validateCustomCss, type CssError } from "../theme/validator";

export type CustomThemeStatus = "absent" | "applied" | "invalid" | "error";

export interface CustomThemeState {
  status: CustomThemeStatus;
  errors: CssError[];
  message: string;
}

export const customTheme = writable<CustomThemeState>({ status: "absent", errors: [], message: "" });

export async function loadCustomTheme(): Promise<CustomThemeState> {
  let state: CustomThemeState;
  try {
    const text = await invokeBackend<string | null>("load_custom_css", { path: getCustomCssPath() });
    if (text === null) {
      applyCustomCss(null);
      state = { status: "absent", errors: [], message: "No ~/.axiomata/theme.css." };
    } else {
      const result = validateCustomCss(text);
      if (result.ok) {
        applyCustomCss(result.css);
        state = { status: "applied", errors: [], message: "Custom CSS applied." };
      } else {
        applyCustomCss(null);
        state = { status: "invalid", errors: result.errors, message: "Custom CSS rejected; built-in theme kept." };
      }
    }
  } catch (err) {
    applyCustomCss(null);
    state = { status: "error", errors: [], message: String(err) };
  }
  customTheme.set(state);
  return state;
}

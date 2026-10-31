/**
 * The Settings dialog's "Web search" block for Glut's `web_search` (`docs/plans/glut-round.md`, W2/W5/W7): the three
 * providers in the order they are asked, and how an edited key field becomes the `KeyUpdate` Rust resolves.
 */

import type { KeyUpdate, WebSettingsUpdate, WebSettingsView } from "./backend";
import { tSettings } from "../i18n/settings";

export type WebKeyId = "exa" | "tinyfish" | "tavily";

export interface WebKeyMeta {
  id: WebKeyId;
  label: string;
  /** What the field says about the provider, in the current language. */
  hint: () => string;
  /** Whether a key is stored, from the redacted view. */
  stored: (view: WebSettingsView) => boolean;
}

/** The providers in the order `web_search` asks them. */
export const WEB_KEYS: readonly WebKeyMeta[] = [
  { id: "exa", label: "Exa", hint: () => tSettings("webExaHint"), stored: (v) => v.has_exa_key },
  { id: "tinyfish", label: "TinyFish", hint: () => tSettings("webTinyfishHint"), stored: (v) => v.has_tinyfish_key },
  { id: "tavily", label: "Tavily", hint: () => tSettings("webTavilyHint"), stored: (v) => v.has_tavily_key },
] as const;

/** A key field the owner did not touch keeps the stored key; an emptied one clears it; a value replaces it. */
export function webKeyUpdate(edits: Partial<Record<WebKeyId, string>>, id: WebKeyId): KeyUpdate {
  if (!(id in edits)) return { kind: "keep" };
  const value = (edits[id] ?? "").trim();
  return value === "" ? { kind: "clear" } : { kind: "set", value };
}

/** The limits a config without `[agents.web]` gets (Rust's defaults): an emptied field falls back to them. */
export const DEFAULT_DAILY_SEARCH_LIMIT = 50;
export const DEFAULT_TAVILY_MONTHLY_LIMIT = 1000;

/**
 * The `agents.web` part of a save: the edited keys and the limits as whole non-negative numbers. An emptied number
 * field (bound as `null`) means the default, never 0 — 0 turns the search off and must be typed on purpose.
 */
export function webUpdate(view: WebSettingsView, edits: Partial<Record<WebKeyId, string>>): WebSettingsUpdate {
  const count = (n: number | null, fallback: number): number =>
    n === null || !Number.isFinite(n) ? fallback : Math.max(0, Math.floor(n));
  return {
    exa_api_key: webKeyUpdate(edits, "exa"),
    tinyfish_api_key: webKeyUpdate(edits, "tinyfish"),
    tavily_api_key: webKeyUpdate(edits, "tavily"),
    daily_search_limit: count(view.daily_search_limit, DEFAULT_DAILY_SEARCH_LIMIT),
    tavily_monthly_limit: count(view.tavily_monthly_limit, DEFAULT_TAVILY_MONTHLY_LIMIT),
  };
}

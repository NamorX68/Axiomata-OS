/**
 * The harnesses a role (chat, skills) can run on, and which providers each serves — the matrix of
 * `docs/plans/harness-choice.md`, mirrored from Rust's `HarnessId::serves`, which stays the authority at save time.
 */

import type { ClaudeLoginStatus, ConfigView, HarnessId, ProviderId } from "./backend";

export type { HarnessId };

export interface HarnessMeta {
  id: HarnessId;
  label: string;
  blurb: string;
}

/**
 * Every harness the Settings dialog offers, in display order. Opencode is no longer one of them
 * (`docs/plans/opencode-exit.md`): a config that still names it reads as the provider's harness of today.
 */
export const HARNESSES: readonly HarnessMeta[] = [
  { id: "claude_code", label: "Claude Code", blurb: "Anthropic only, on the app's own login" },
  { id: "axh", label: "Glut", blurb: "Axiomata's own harness — OpenRouter or Ollama" },
] as const;

/** Whether `harness` can drive turns of `provider`. */
export function serves(harness: HarnessId, provider: ProviderId): boolean {
  switch (harness) {
    case "opencode":
      // Left (docs/plans/opencode-exit.md): no role runs on it.
      return false;
    case "claude_code":
      return provider === "anthropic";
    case "axh":
      return provider !== "anthropic";
  }
}

/** The providers of `all` that `harness` serves, in their order. */
export function providersFor<T extends { id: ProviderId }>(harness: HarnessId, all: readonly T[]): T[] {
  return all.filter((p) => serves(harness, p.id));
}

/**
 * The provider a role keeps after its harness changed to `harness`: the current one when the harness serves it,
 * else the first that it does, in `order`.
 */
export function providerAfterHarnessChange(
  harness: HarnessId,
  current: ProviderId,
  order: readonly ProviderId[],
): ProviderId {
  if (serves(harness, current)) return current;
  return order.find((p) => serves(harness, p)) ?? current;
}

/** Whether either role runs on Claude Code — then its sign-in matters. */
export function usesClaudeCode(chat: HarnessId, skills: HarnessId): boolean {
  return chat === "claude_code" || skills === "claude_code";
}

/** What the start notice tells the owner when a role runs on Claude Code but the app's Claude Code is not signed in. */
export const CLAUDE_SIGN_IN_NOTICE =
  "Claude Code is not signed in — Settings → AI providers → “Sign in with Anthropic”.";

/**
 * The notice to show at app start, or `null`: only when a role runs on Claude Code and its sign-in is missing. A
 * failing look says nothing — the Settings dialog shows the reason when it is opened.
 */
export async function claudeSignInNotice(
  loadConfig: () => Promise<ConfigView>,
  loadStatus: () => Promise<ClaudeLoginStatus>,
): Promise<string | null> {
  try {
    const config = await loadConfig();
    if (!usesClaudeCode(config.agents.chat_harness, config.agents.skill_harness)) return null;
    const status = await loadStatus();
    return status.logged_in ? null : CLAUDE_SIGN_IN_NOTICE;
  } catch {
    return null;
  }
}

/** A backend id as the dashboard names it: a harness's label (Glut for `axh`), any other id (`ollama`) as it is. */
export function backendLabel(id: string): string {
  return HARNESSES.find((h) => h.id === id)?.label ?? id;
}

/** Every backend a skill or routine may name, for pickers and filters: the three harnesses, then `ollama`. */
export const BACKEND_CHOICES: readonly { id: string; label: string }[] = [
  ...HARNESSES.map((h) => ({ id: h.id, label: h.label })),
  { id: "ollama", label: "Ollama (no tools)" },
];

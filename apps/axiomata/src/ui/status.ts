/**
 * The tones of a status mark (`ui/StatusMark.svelte`) and of a notice (`ui/Notice.svelte`) — the app's one
 * vocabulary for "how is this thing doing", so an agent, a column and a notice cannot drift into colour schemes of
 * their own.
 */

/**
 * * `neutral` — nothing to say (open, ready);
 * * `live` — something works here: the glow (accent rule);
 * * `ok` — done, checked, idle and fine;
 * * `warn` — waits for the owner (a question, a review);
 * * `bad` — failed;
 * * `off` — over, closed;
 * * `unknown` — no channel to know it by.
 */
export type StatusTone = "neutral" | "live" | "ok" | "warn" | "bad" | "off" | "unknown";

/** An agent's state (`core/backend.ts` `AgentState`, plus the views' own `none`) as a tone. */
export function agentTone(state: string): StatusTone {
  switch (state) {
    case "working":
      return "live";
    case "waiting":
      return "warn";
    case "idle":
      return "ok";
    case "starting":
      return "neutral";
    case "ended":
      return "off";
    default:
      return "unknown";
  }
}

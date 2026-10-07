/**
 * How a card session's usage is shown (A2A CP-A6c): one line per limit, with what is used of what is allowed. Pure, so
 * it is tested without a backend.
 */
import type { SessionUsage, Unmeasured } from "../core/backend";

/** A token count the way a limit is spoken of: `950`, `340 k`, `1,5 M`. */
export function formatTokens(count: number): string {
  if (count >= 1_000_000) return `${(count / 1_000_000).toLocaleString("de-DE", { maximumFractionDigits: 1 })} M`;
  if (count >= 1_000) return `${Math.round(count / 1_000)} k`;
  return String(count);
}

/** One limit of a session: what it is called, what is used, what is allowed, and whether it is used up. */
export interface UsageLine {
  label: string;
  used: string;
  allowed: string;
  /** 0–1, for a bar; the limit counts as reached at 1. */
  share: number;
  reached: boolean;
}

function line(label: string, used: number, allowed: number, show: (n: number) => string): UsageLine {
  const share = allowed > 0 ? Math.min(used / allowed, 1) : 0;
  return { label, used: show(used), allowed: show(allowed), share, reached: used >= allowed };
}

/**
 * The lines of a session: steps, tokens and — only where money was metered — dollars. A subscription engine or a model
 * without a price in the owner's table has no money line: a figure there would be made up.
 */
export function usageLines(session: SessionUsage): UsageLine[] {
  const lines = [
    line("Steps", session.usage.steps, session.limits.max_steps, String),
    line(
      "Tokens",
      session.usage.input_tokens + session.usage.output_tokens,
      session.limits.max_tokens,
      formatTokens,
    ),
  ];
  if (session.cost_usd !== null) {
    lines.push(line("Cost", session.cost_usd, session.limits.max_cost_usd, (n) => `$${n.toFixed(2)}`));
  }
  return lines;
}

const UNMEASURED: Record<Unmeasured, string> = {
  no_session_id: "No Claude Code session was recorded for this session (it ran with its own command).",
  no_record_yet: "The session has not recorded anything yet — it may have just started.",
  read_failed: "The session's record could not be read.",
  no_opencode_session: "The Opencode session has not been created yet.",
  service_down: "The Opencode service is not running.",
  service_failed: "The Opencode service did not answer.",
  not_measured: "This harness is not measured.",
};

/**
 * Why a session's figures are not to be believed, in words — and what that means for its limit. Only the one that is
 * about to matter is said: while the record is simply not there yet nothing has been used, and a limit starts to count
 * as soon as there is one.
 */
export function unmeasuredNote(session: Pick<SessionUsage, "measured" | "unmeasured">): string | null {
  if (session.measured) return null;
  const why = UNMEASURED[session.unmeasured ?? "read_failed"];
  return `${why} A limit only applies once the session can be measured.`;
}

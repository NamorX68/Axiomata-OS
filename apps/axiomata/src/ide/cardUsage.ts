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
    line("Schritte", session.usage.steps, session.limits.max_steps, String),
    line(
      "Token",
      session.usage.input_tokens + session.usage.output_tokens,
      session.limits.max_tokens,
      formatTokens,
    ),
  ];
  if (session.cost_usd !== null) {
    lines.push(line("Kosten", session.cost_usd, session.limits.max_cost_usd, (n) => `$${n.toFixed(2)}`));
  }
  return lines;
}

const UNMEASURED: Record<Unmeasured, string> = {
  no_session_id: "Für diese Sitzung wurde keine Claude-Code-Sitzung vermerkt (sie lief mit eigenem Befehl).",
  no_record_yet: "Die Sitzung hat noch nichts aufgezeichnet — sie hat vielleicht eben erst gestartet.",
  read_failed: "Die Aufzeichnung der Sitzung ließ sich nicht lesen.",
  no_opencode_session: "Die Opencode-Sitzung ist noch nicht angelegt.",
  service_down: "Der Opencode-Dienst läuft nicht.",
  service_failed: "Der Opencode-Dienst hat nicht geantwortet.",
  not_measured: "Dieses Harness wird nicht gemessen.",
};

/**
 * Why a session's figures are not to be believed, in words — and what that means for its limit. Only the one that is
 * about to matter is said: while the record is simply not there yet nothing has been used, and a limit starts to count
 * as soon as there is one.
 */
export function unmeasuredNote(session: Pick<SessionUsage, "measured" | "unmeasured">): string | null {
  if (session.measured) return null;
  const why = UNMEASURED[session.unmeasured ?? "read_failed"];
  return `${why} Ein Limit greift erst, wenn sich die Sitzung messen lässt.`;
}

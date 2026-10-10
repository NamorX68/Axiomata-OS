/**
 * How the Inbox tab words a session's MCP entry (A2A CP-A5): pure, so the wording is tested without a component.
 */
import type { AgentEntry } from "../core/backend";
import type { ReachView } from "./reach";

export type EntryTone = "ok" | "warn" | "none";

export interface EntryView {
  label: string;
  tone: EntryTone;
  /** One sentence under the label; empty when the label says it all. */
  detail: string;
}

/**
 * What the owner reads about the entry. `null` is a session that is still being prepared. `reach` is what the pane
 * found out later: a registered entry only counts as connected while the server is really running (card #72).
 */
export function describeEntry(
  entry: AgentEntry | null,
  reach: ReachView = "unknown",
  reason: string | null = null,
): EntryView {
  if (!entry) return { label: "Preparing…", tone: "none", detail: "" };
  switch (entry.status) {
    case "registered":
      if (reach === "reconnecting") {
        return { label: "Reconnecting…", tone: "none", detail: "Registering the team tools with the service again." };
      }
      if (reach === "unreachable") {
        return {
          label: "Not reachable",
          tone: "warn",
          detail:
            "The service dropped this session's team tools while it sat idle, so mail to it comes back undeliverable." +
            (reason ? ` ${reason}` : ""),
        };
      }
      return {
        label: "Team tools connected",
        tone: "ok",
        detail: "This session can reach the other sessions and the board through the tools below.",
      };
    case "unavailable":
      return {
        label: "Team tools unavailable",
        tone: "warn",
        detail: entry.note ?? "The session runs without them.",
      };
    case "not_applicable":
      return { label: "Team tools off", tone: "none", detail: entry.note ?? "" };
  }
}

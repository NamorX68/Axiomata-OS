/**
 * How the Inbox tab words a session's MCP entry (A2A CP-A5): pure, so the wording is tested without a component.
 */
import type { AgentEntry } from "../core/backend";

export type EntryTone = "ok" | "warn" | "none";

export interface EntryView {
  label: string;
  tone: EntryTone;
  /** One sentence under the label; empty when the label says it all. */
  detail: string;
}

/** What the owner reads about the entry. `null` is a session that is still being prepared. */
export function describeEntry(entry: AgentEntry | null): EntryView {
  if (!entry) return { label: "Preparing…", tone: "none", detail: "" };
  switch (entry.status) {
    case "registered":
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

/**
 * How the Inbox tab words a session's MCP entry (A2A CP-A5): pure, so the wording is tested without a component.
 */
import type { AgentEntry } from "../core/backend";
import { tFlow } from "../i18n/flow";

export type EntryTone = "ok" | "warn" | "none";

export interface EntryView {
  label: string;
  tone: EntryTone;
  /** One sentence under the label; empty when the label says it all. */
  detail: string;
}

/** What the owner reads about the entry. `null` is a session that is still being prepared. */
export function describeEntry(entry: AgentEntry | null): EntryView {
  if (!entry) return { label: tFlow("entryPreparing"), tone: "none", detail: "" };
  switch (entry.status) {
    case "registered":
      return {
        label: tFlow("entryConnected"),
        tone: "ok",
        detail: tFlow("entryConnectedDetail"),
      };
    case "unavailable":
      return {
        label: tFlow("entryUnavailable"),
        tone: "warn",
        detail: entry.note ?? tFlow("entryUnavailableDetail"),
      };
    case "not_applicable":
      return { label: tFlow("entryOff"), tone: "none", detail: entry.note ?? "" };
  }
}

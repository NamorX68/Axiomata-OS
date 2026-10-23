import { describe, expect, it } from "vitest";
import type { AgentEntry } from "../core/backend";
import { describeEntry } from "./agentEntry";

const entry = (over: Partial<AgentEntry>): AgentEntry => ({
  status: "registered",
  server: "axiomata",
  command: "/opt/ax/axiomata-cli",
  tools: ["read_inbox"],
  note: null,
  ...over,
});

describe("describeEntry", () => {
  it("says a session is still being prepared", () => {
    expect(describeEntry(null)).toEqual({ label: "Preparing…", tone: "none", detail: "" });
  });

  it("reads a wired-in session as connected", () => {
    const view = describeEntry(entry({}));
    expect(view.label).toBe("Team tools connected");
    expect(view.tone).toBe("ok");
  });

  it("carries the reason when the entry could not be made", () => {
    const view = describeEntry(entry({ status: "unavailable", note: "no axiomata-cli was found", tools: [] }));
    expect(view.tone).toBe("warn");
    expect(view.detail).toBe("no axiomata-cli was found");
  });

  it("has an answer even when the backend gave no reason", () => {
    expect(describeEntry(entry({ status: "unavailable", note: null })).detail).not.toBe("");
    expect(describeEntry(entry({ status: "not_applicable", note: null })).detail).toBe("");
  });

  it("is off, not an error, for a session on a command of its own", () => {
    const view = describeEntry(entry({ status: "not_applicable", note: "runs a command of its own" }));
    expect(view).toEqual({ label: "Team tools off", tone: "none", detail: "runs a command of its own" });
  });
});

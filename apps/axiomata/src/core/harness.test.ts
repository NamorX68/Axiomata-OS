import { describe, expect, it } from "vitest";

import type { ClaudeLoginStatus, ConfigView, HarnessId, ProviderId } from "./backend";
import {
  BACKEND_CHOICES,
  CLAUDE_SIGN_IN_NOTICE,
  HARNESSES,
  backendLabel,
  claudeSignInNotice,
  providerAfterHarnessChange,
  providersFor,
  serves,
  usesClaudeCode,
} from "./harness";

const ORDER: ProviderId[] = ["anthropic", "open_router", "ollama"];

describe("harness matrix", () => {
  it("lets Claude Code serve only Anthropic and Glut all but Anthropic", () => {
    for (const p of ORDER) {
      expect(serves("claude_code", p)).toBe(p === "anthropic");
      expect(serves("axh", p)).toBe(p !== "anthropic");
    }
  });

  it("offers each harness only the providers it serves, in their order", () => {
    const all = ORDER.map((id) => ({ id }));
    expect(providersFor("claude_code", all).map((p) => p.id)).toEqual(["anthropic"]);
    expect(providersFor("axh", all).map((p) => p.id)).toEqual(["open_router", "ollama"]);
  });

  it("keeps a role's provider when the new harness serves it and moves it to the first served one otherwise", () => {
    expect(providerAfterHarnessChange("claude_code", "open_router", ORDER)).toBe("anthropic");
    expect(providerAfterHarnessChange("axh", "anthropic", ORDER)).toBe("open_router");
    expect(providerAfterHarnessChange("axh", "ollama", ORDER)).toBe("ollama");
  });

  it("names Claude Code's sign-in as needed when either role runs on it", () => {
    expect(usesClaudeCode("claude_code", "axh")).toBe(true);
    expect(usesClaudeCode("axh", "claude_code")).toBe(true);
    expect(usesClaudeCode("axh", "axh")).toBe(false);
  });

  it("lists Claude Code and Glut, with Glut as the own one", () => {
    expect(HARNESSES.map((h) => h.id)).toEqual(["claude_code", "axh"]);
    expect(HARNESSES.find((h) => h.id === "axh")?.label).toBe("Glut");
  });
});

describe("the start notice for Claude Code's sign-in", () => {
  const config = (chat: HarnessId, skill: HarnessId) => async () =>
    ({ agents: { chat_harness: chat, skill_harness: skill } }) as unknown as ConfigView;
  const status = (logged_in: boolean) => async (): Promise<ClaudeLoginStatus> => ({
    logged_in,
    email: null,
    subscription: null,
  });

  it("speaks only when a role runs on Claude Code and its sign-in is missing", async () => {
    expect(await claudeSignInNotice(config("claude_code", "axh"), status(false))).toBe(CLAUDE_SIGN_IN_NOTICE);
    expect(await claudeSignInNotice(config("claude_code", "axh"), status(true))).toBeNull();
    expect(await claudeSignInNotice(config("axh", "axh"), status(false))).toBeNull();
  });

  it("does not ask for the sign-in when no role needs it, and stays silent when a look fails", async () => {
    let asked = false;
    await claudeSignInNotice(config("axh", "axh"), async () => {
      asked = true;
      return { logged_in: false, email: null, subscription: null };
    });
    expect(asked).toBe(false);
    const failing = async (): Promise<ClaudeLoginStatus> => {
      throw new Error("no claude");
    };
    expect(await claudeSignInNotice(config("claude_code", "axh"), failing)).toBeNull();
  });
});

describe("backend names in the dashboard", () => {
  it("shows a harness by its label and any other backend as it is", () => {
    expect(backendLabel("axh")).toBe("Glut");
    expect(backendLabel("claude_code")).toBe("Claude Code");
    expect(backendLabel("ollama")).toBe("ollama");
  });

  it("offers the two harnesses and ollama as backends", () => {
    expect(BACKEND_CHOICES.map((c) => c.id)).toEqual(["claude_code", "axh", "ollama"]);
  });
});

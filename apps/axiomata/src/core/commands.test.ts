import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

import { registerBuiltins } from "../modules";
import {
  type EngineCommand,
  chatHarness,
  commandEntries,
  engineCommands,
  nearest,
  refreshChatHarness,
  refreshEngineCommands,
  route,
  runCommand,
  suggestions,
} from "./commands";
import { on } from "./bus";
import { busy, newSession, planMode, sessionId, turnEffort, turnModel, turns } from "./chat";
import { modelCatalog } from "./modelCatalog";
import { activeTheme, instances, loadInstances } from "./stores";
import { staged } from "./staging";

beforeAll(() => registerBuiltins());
beforeEach(() => {
  loadInstances([]);
  staged.set([]);
  turns.set([]);
  planMode.set(false);
});

const ENGINE: EngineCommand[] = [
  { name: "review", source: "oc", description: "review changes" },
  { name: "init", source: "oc", description: "guided AGENTS.md setup" },
  { name: "help", source: "oc", description: "the engine's help" },
  { name: "apple-reminders:weekly-planning-workflow", source: "oc", description: "a prompt" },
];

describe("route", () => {
  it("sends plain text to the chat and /do to a one-shot instruction", () => {
    expect(route("   ")).toBeNull();
    expect(route("hello there")).toEqual({ kind: "chat", message: "hello there" });
    expect(route("/do schreib Milch auf die Liste")).toEqual({
      kind: "instruct",
      message: "schreib Milch auf die Liste",
    });
    expect(route("/do")).toMatchObject({ kind: "unknown", message: "usage: /do <text>" });
  });

  it("knows the app's two levels, case-insensitively", () => {
    expect(route("/THEME ocean")).toEqual({ kind: "command", name: "theme", args: ["ocean"] });
    expect(route("/tile memory-status status")).toEqual({
      kind: "command",
      name: "tile",
      args: ["memory-status", "status"],
    });
    expect(route("/clear")).toEqual({ kind: "command", name: "clear", args: [] });
  });

  it("refuses an unknown /x instead of spending a turn on it, naming the nearest commands", () => {
    const typo = route("/thme ocean");
    expect(typo).toMatchObject({ kind: "unknown", name: "thme" });
    expect(typo?.kind === "unknown" && typo.message).toMatch(/did you mean \/theme/);
    const nothing = route("/schreib Milch auf die Liste");
    expect(nothing?.kind === "unknown" && nothing.message).toMatch(/\/do <text>/);
  });

  it("points the old spellings at their new place", () => {
    const hint = (input: string): string => {
      const r = route(input);
      return r?.kind === "unknown" ? r.message : "";
    };
    expect(hint("/add todo")).toBe("/add is now /tile add");
    expect(hint("/skill run news-web")).toBe("/skill is now /skills run");
    expect(hint("/memory-status sync")).toBe("/memory-status is now /tile memory-status");
  });

  it("routes an engine's command under its own name and keeps the rest of the line as typed", () => {
    expect(route("/review  the branch  now", ENGINE)).toEqual({
      kind: "engine",
      name: "review",
      source: "oc",
      args: "the branch  now",
    });
    expect(route("/apple-reminders:weekly-planning-workflow", ENGINE)).toMatchObject({
      kind: "engine",
      name: "apple-reminders:weekly-planning-workflow",
    });
  });

  it("gives a clash to the app and the engine's command a prefix", () => {
    expect(route("/help", ENGINE)).toEqual({ kind: "command", name: "help", args: [] });
    expect(route("/oc:help", ENGINE)).toMatchObject({ kind: "engine", name: "help", source: "oc" });
    expect(route("/oc:init", ENGINE)).toMatchObject({ kind: "engine", name: "init" });
    // A prefix names one engine: another's prefix does not reach it.
    expect(route("/cc:review", ENGINE)).toMatchObject({ kind: "unknown" });
  });
});

describe("the command list", () => {
  it("lists the app's commands first and prefixes only the clashing engine names", () => {
    const names = commandEntries(ENGINE).map((c) => c.name);
    expect(names.slice(0, 2)).toEqual(["help", "clear"]);
    expect(names).toContain("review");
    expect(names).toContain("oc:help");
    expect(names).not.toContain("add");
  });

  it("suggests while the first word is typed, and stops at the first space", () => {
    expect(suggestions("/c", ENGINE).map((c) => c.name)).toEqual(["clear", "compact", "cost"]);
    expect(suggestions("/oc:", ENGINE).map((c) => c.name)).toEqual(["oc:help"]);
    expect(suggestions("/", ENGINE).length).toBe(commandEntries(ENGINE).length);
    expect(suggestions("/theme ", ENGINE)).toEqual([]);
    expect(suggestions("theme", ENGINE)).toEqual([]);
  });

  it("offers /effort only while the chat runs on Glut, and everything while the harness is unknown", () => {
    const names = (harness: "axh" | "claude_code" | null) => commandEntries([], harness).map((c) => c.name);
    expect(names("axh")).toContain("effort");
    expect(names("claude_code")).not.toContain("effort");
    expect(names("claude_code")).toContain("model");
    expect(names(null)).toContain("effort");
    chatHarness.set("claude_code");
    expect(suggestions("/eff", [])).toEqual([]);
    chatHarness.set(null);
    expect(suggestions("/eff", []).map((c) => c.name)).toEqual(["effort"]);
  });

  it("finds near names by prefix first, then by a small edit distance", () => {
    expect(nearest("co", ["clear", "compact", "cost"])).toEqual(["compact", "cost"]);
    expect(nearest("modle", ["model", "mode-x", "theme"])).toEqual(["model"]);
    expect(nearest("zzzzzz", ["model"])).toEqual([]);
  });

  it("/help lists the three levels and the placeable tile types in the panel, read when asked", async () => {
    engineCommands.set(ENGINE);
    const panel = (await runCommand("help", [])).panel;
    const cells = panel?.rows.map((r) => r.cells.join(" | ")) ?? [];
    expect(panel?.rows.map((r) => r.group)).toContain("Conversation");
    const tile = cells.find((c) => c.startsWith("/tile add|remove <type>"));
    expect(tile).toContain("memory-status");
    expect(tile).not.toMatch(/\bfile\b/);
    expect(cells).toContain("/oc:help | the engine's help | oc");
    engineCommands.set([]);
    const later = (await runCommand("help", [])).panel?.rows.map((r) => r.cells[0]);
    expect(later).toContain("its commands appear once the chat knows them");
  });
});

describe("runCommand", () => {
  it("/help opens the command list in the panel, never in the conversation", async () => {
    const r = await runCommand("help", []);
    expect(r.ok).toBe(true);
    expect(r.panel?.kind).toBe("list");
    const tile = r.panel?.rows.find((row) => row.cells[0]?.startsWith("/tile"));
    expect(tile).toMatchObject({ line: "/tile ", run: false });
  });

  it("/help <topic> opens that handbook page, an unknown topic lists the pages", async () => {
    const opened: unknown[] = [];
    const off = on("shell:handbook", (detail) => opened.push(detail));
    try {
      const found = await runCommand("help", ["Studio"]);
      expect(found.ok).toBe(true);
      expect(opened).toEqual([{ page: "studio" }]);
      const unknown = await runCommand("help", ["nothing", "here"]);
      expect(unknown.ok).toBe(false);
      expect(unknown.panel?.rows).toContainEqual(expect.objectContaining({ line: "/help kanban", run: true }));
      expect(opened).toHaveLength(1);
    } finally {
      off();
    }
  });

  it("/clear starts a new conversation, /plan and /build flip the switch", async () => {
    sessionId.set("s-1");
    turns.set([{ id: 1, role: "user", text: "hi", at: 0 }]);
    await runCommand("clear", []);
    expect(get(sessionId)).toBeNull();
    expect(get(turns)).toEqual([]);
    await runCommand("plan", []);
    expect(get(planMode)).toBe(true);
    await runCommand("build", []);
    expect(get(planMode)).toBe(false);
  });

  it("/cost adds the replies' prices and says when some had none", async () => {
    expect((await runCommand("cost", [])).message).toMatch(/No turns/);
    turns.set([
      { id: 1, role: "user", text: "a", at: 0 },
      { id: 2, role: "assistant", text: "b", at: 0, costUsd: 0.0125 },
      { id: 3, role: "assistant", text: "c", at: 0, costUsd: null },
    ]);
    const cost = await runCommand("cost", []);
    expect(cost.panel?.kind).toBe("info");
    expect(cost.panel?.rows.map((r) => r.cells)).toEqual([
      ["Turns", "2"],
      ["Cost", "$0.0125"],
      ["Without a price", "1"],
    ]);
  });

  it("/skills lists the skills, /skills run runs one", async () => {
    const backend = await import("./backend");
    const spy = vi.spyOn(backend, "invokeBackend");
    spy.mockResolvedValueOnce([{ name: "news-web", description: "the news", backend: null, harness: "axh" }]);
    const list = await runCommand("skills", []);
    expect(list.panel?.rows[0]).toMatchObject({
      cells: ["news-web", "the news", "axh"],
      line: "/skills run news-web",
      confirm: "Run news-web? ⏎ yes · Esc no",
    });
    spy.mockResolvedValueOnce({ status: "success", duration_ms: 12 });
    expect(await runCommand("skills", ["run", "news-web"])).toMatchObject({
      ok: true,
      message: "/news-web: success (12 ms)",
    });
    expect(spy).toHaveBeenLastCalledWith("run_skill", { name: "news-web" });
    expect((await runCommand("skills", ["run"])).ok).toBe(false);
    expect((await runCommand("skills", ["nope"])).ok).toBe(false);
    spy.mockRestore();
  });

  it("/tile add and /tile remove manage tiles", async () => {
    expect((await runCommand("tile", ["add", "memory-status"])).ok).toBe(true);
    expect(get(instances)).toHaveLength(1);
    const bad = await runCommand("tile", ["add", "nope"]);
    expect(bad.ok).toBe(false);
    expect(bad.message).toMatch(/unknown module type/);
    expect((await runCommand("tile", ["remove", "memory-status"])).ok).toBe(true);
    expect(get(instances)).toHaveLength(0);
    expect((await runCommand("tile", ["remove", "memory-status"])).ok).toBe(false);
    expect((await runCommand("tile", [])).message).toMatch(/usage: \/tile/);
  });

  it("/theme switches known themes only", async () => {
    document.documentElement.dataset.theme = "graphite";
    expect((await runCommand("theme", ["ocean"])).ok).toBe(true);
    expect(get(activeTheme)).toBe("ocean");
    expect(document.documentElement.dataset.theme).toBe("ocean");
    const bad = await runCommand("theme", ["neon"]);
    expect(bad.ok).toBe(false);
    expect(get(activeTheme)).toBe("ocean");
  });

  it("/open stages a markdown panel", async () => {
    expect((await runCommand("open", [])).ok).toBe(false);
    expect((await runCommand("open", ["notes/inbox.md"])).ok).toBe(true);
    expect(get(staged)).toMatchObject([{ type: "file", config: { path: "notes/inbox.md", mode: "read" } }]);
  });

  it("/newfile creates an empty file and opens it in edit mode, not read mode", async () => {
    expect((await runCommand("newfile", [])).ok).toBe(false);
    const r = await runCommand("newfile", ["notes/new.md"]);
    expect(r.ok).toBe(true);
    expect(get(staged)).toMatchObject([{ type: "file", config: { path: "notes/new.md", mode: "edit" } }]);
  });

  it("/newfile reports the backend's failure instead of opening a panel", async () => {
    const spy = vi.spyOn(await import("./backend"), "invokeBackend").mockRejectedValueOnce(new Error("disk full"));
    const r = await runCommand("newfile", ["notes/broken.md"]);
    expect(r.ok).toBe(false);
    expect(r.message).toMatch(/disk full/);
    expect(get(staged)).toHaveLength(0);
    spy.mockRestore();
  });

  it("/tile <type> <action> needs a mounted instance and valid JSON params", async () => {
    const none = await runCommand("tile", ["memory-status", "status"]);
    expect(none.ok).toBe(false);
    expect(none.message).toMatch(/\/tile add memory-status/);
    await runCommand("tile", ["add", "memory-status"]);
    const usage = await runCommand("tile", ["memory-status"]);
    expect(usage.ok).toBe(false);
    expect(usage.message).toMatch(/— sync, status/);
    const badJson = await runCommand("tile", ["memory-status", "status", "{nope"]);
    expect(badJson.ok).toBe(false);
    const ok = await runCommand("tile", ["memory-status", "status"]);
    expect(ok.ok).toBe(true);
    expect(ok.panel).toMatchObject({ kind: "info", code: true });
    expect(ok.panel?.text).toContain("tracked_files");
    const unknown = await runCommand("tile", ["memory-status", "explode"]);
    expect(unknown.ok).toBe(false);
    expect(unknown.message).toMatch(/no action "explode"/);
    expect((await runCommand("tile", ["frobnicate", "x"])).message).toMatch(/unknown tile type/);
  });

  it("unknown commands are reported", async () => {
    const r = await runCommand("frobnicate", []);
    expect(r.ok).toBe(false);
  });
});

describe("the conversation commands that reach the backend (#108)", () => {
  const config = {
    owner: "o",
    workspace_root: "/w",
    agents: {
      chat_provider: "open_router",
      providers: { open_router: { base_url: null, has_key: true, chat_model: "configured/model", skill_model: "s" } },
    },
  };

  beforeEach(() => {
    sessionId.set(null);
    turnModel.set(null);
    busy.set(false);
    modelCatalog.set({
      providers: {
        open_router: {
          models: [
            { id: "allowed/one", name: "One", context_length: 128000 },
            { id: "allowed/two", name: "allowed/two" },
          ],
        },
      },
    } as never);
  });

  async function backend() {
    const module = await import("./backend");
    return vi.spyOn(module, "invokeBackend");
  }

  it("/model shows the models, switches this conversation to an allowed one and goes back", async () => {
    const spy = await backend();
    spy.mockResolvedValue(config);
    const shown = await runCommand("model", []);
    expect(shown.panel?.note).toBe("configured/model, the configured chat model of open_router");
    expect(shown.panel?.rows[0]).toMatchObject({ cells: ["One · allowed/one", "—", "128k"], line: "/model allowed/one" });
    expect(shown.panel?.takesTyped?.("some/model")).toBe("/model some/model");
    expect(shown.panel?.takesTyped?.("-p")).toBeNull();
    expect((await runCommand("model", ["not/listed"])).message).toMatch(/not among open_router's models/);
    expect((await runCommand("model", ["-p"])).ok).toBe(false);
    expect((await runCommand("model", ["allowed/two"])).ok).toBe(true);
    expect(get(turnModel)).toBe("allowed/two");
    const again = (await runCommand("model", [])).panel;
    expect(again?.note).toContain("allowed/two for this conversation");
    expect(again?.rows.find((r) => r.current)?.id).toBe("allowed/two");
    expect(again?.rows[0]?.line).toBe("/model default");
    await runCommand("model", ["default"]);
    expect(get(turnModel)).toBeNull();
    spy.mockRestore();
  });

  it("/effort is Glut's: it switches this conversation's level, goes back, and refuses elsewhere", async () => {
    const spy = await backend();
    const glut = { ...config, agents: { ...config.agents, chat_harness: "axh", chat_effort: "medium" } };
    spy.mockResolvedValue(glut);
    turnEffort.set(null);
    expect((await runCommand("effort", [])).panel?.note).toContain("medium, the configured chat effort");
    expect((await runCommand("effort", ["hard"])).message).toMatch(/no effort level/);
    expect(get(turnEffort)).toBeNull();
    expect((await runCommand("effort", ["XHigh"])).ok).toBe(true);
    expect(get(turnEffort)).toBe("xhigh");
    expect((await runCommand("effort", [])).panel?.note).toContain("xhigh for this conversation");
    await runCommand("effort", ["default"]);
    expect(get(turnEffort)).toBeNull();
    turnEffort.set("low");
    newSession();
    expect(get(turnEffort), "a new conversation goes back to the configured level").toBeNull();
    spy.mockResolvedValue({ ...config, agents: { ...config.agents, chat_harness: "opencode" } });
    const elsewhere = await runCommand("effort", ["high"]);
    expect(elsewhere.ok).toBe(false);
    expect(get(turnEffort)).toBeNull();
    spy.mockRestore();
  });

  it("/effort lists the chat model's own levels, marks the current one and notes one it does not list", async () => {
    const spy = await backend();
    spy.mockResolvedValue({ ...config, agents: { ...config.agents, chat_harness: "axh", chat_effort: null } });
    modelCatalog.set({
      providers: {
        open_router: {
          models: [
            {
              id: "configured/model",
              name: "Flash",
              reasoning: { efforts: ["none", "low", "high", "max"], default_effort: "high", mandatory: false },
            },
          ],
        },
      },
    } as never);
    turnEffort.set(null);
    const shown = (await runCommand("effort", [])).panel;
    expect(shown?.note).toContain("model's default (high)");
    expect(shown?.rows.map((r) => r.id)).toEqual(["none", "low", "high", "max"]);
    await runCommand("effort", ["low"]);
    const marked = (await runCommand("effort", [])).panel;
    expect(marked?.rows.find((r) => r.current)?.id).toBe("low");
    expect(marked?.rows[0]).toMatchObject({ cells: ["default (high)"], line: "/effort default" });
    const unlisted = await runCommand("effort", ["medium"]);
    expect(unlisted.ok).toBe(true);
    expect(unlisted.message).toMatch(/does not list medium/);
    expect((await runCommand("effort", ["hard"])).message).toBe("“hard” is no effort level — none, low, high, max");
    expect((await runCommand("effort", ["default"])).message).toBe("Effort: back to default (high).");
    expect(get(turnEffort)).toBeNull();
    spy.mockRestore();
  });

  it("reads the chat's harness for the harness-only commands and keeps it when the backend cannot answer", async () => {
    const spy = await backend();
    spy.mockResolvedValueOnce({ ...config, agents: { ...config.agents, chat_harness: "opencode" } });
    await refreshChatHarness();
    expect(get(chatHarness)).toBe("opencode");
    spy.mockRejectedValueOnce(new Error("not up"));
    await refreshChatHarness();
    expect(get(chatHarness)).toBe("opencode");
    chatHarness.set(null);
    spy.mockRestore();
  });

  it("/compact, /resume and /undo wait while the agent works", async () => {
    busy.set(true);
    sessionId.set("ses_1");
    for (const name of ["compact", "resume", "undo", "clear"]) {
      expect((await runCommand(name, [])).message).toMatch(/agent is working/);
    }
    expect(get(sessionId)).toBe("ses_1");
    busy.set(false);
  });

  it("an engine command is matched without case and keeps the engine's spelling", () => {
    const engine: EngineCommand[] = [{ name: "Review", source: "cc", description: "command" }];
    expect(route("/review now", engine)).toMatchObject({ kind: "engine", name: "Review", args: "now" });
    expect(route("/CC:REVIEW", engine)).toMatchObject({ kind: "engine", name: "Review" });
  });

  it("/resume shows a harness's title as text in its row, never as markup", async () => {
    const spy = await backend();
    const title = "[click](http://x) `code`";
    spy.mockResolvedValue([{ id: "ses_x", title, updated_at: "2026-10-20T08:00:00Z", turns: 1 }]);
    expect((await runCommand("resume", [])).panel?.rows[0]?.cells[0]).toBe(title);
    spy.mockRestore();
  });

  it("/compact needs a conversation and sends a compaction turn", async () => {
    const spy = await backend();
    expect((await runCommand("compact", [])).message).toMatch(/Nothing to compact/);
    sessionId.set("ses_1");
    const compacted = { session_id: "ses_1", reply_markdown: "Conversation compacted.", is_error: false };
    spy.mockResolvedValue({ ...compacted, cost_usd: null });
    expect((await runCommand("compact", [])).ok).toBe(true);
    expect(spy).toHaveBeenCalledWith("assistant_send", expect.objectContaining({ command: { kind: "compact" } }));
    spy.mockRestore();
  });

  it("/resume lists the earlier conversations and goes on with a listed one only", async () => {
    const spy = await backend();
    spy.mockResolvedValue([{ id: "ses_old", title: "fix the tests", updated_at: "2026-10-20T08:12:00Z", turns: 3 }]);
    const listed = await runCommand("resume", []);
    expect(listed.panel?.rows[0]).toMatchObject({ line: "/resume ses_old", run: true });
    expect(listed.panel?.rows[0]?.cells[0]).toBe("fix the tests");
    expect(listed.panel?.rows[0]?.cells[2]).toBe("3 turns");
    expect((await runCommand("resume", ["ses_other"])).ok).toBe(false);
    turns.set([{ id: 1, role: "user", text: "now", at: 0 }]);
    expect((await runCommand("resume", ["ses_old"])).ok).toBe(true);
    expect(get(sessionId)).toBe("ses_old");
    expect(get(turns)).toEqual([]);
    spy.mockRestore();
  });

  it("/undo needs a conversation and says what its harness did", async () => {
    const spy = await backend();
    expect((await runCommand("undo", [])).ok).toBe(false);
    sessionId.set("axh-1");
    spy.mockResolvedValueOnce("Undid the file changes of the last turn.");
    expect(await runCommand("undo", [])).toMatchObject({
      ok: true,
      panel: { kind: "info", text: "Undid the file changes of the last turn." },
    });
    expect(spy).toHaveBeenLastCalledWith("chat_undo", { sessionId: "axh-1" });
    spy.mockRejectedValueOnce("Claude Code cannot undo");
    expect((await runCommand("undo", [])).ok).toBe(false);
    spy.mockRestore();
  });

  it("reads the engine's commands, and keeps the last list when the harness cannot answer", async () => {
    const spy = await backend();
    spy.mockResolvedValueOnce([{ name: "review", source: "oc", hint: null, description: "review changes" }]);
    await refreshEngineCommands();
    expect(get(engineCommands)).toEqual([{ name: "review", source: "oc", description: "review changes" }]);
    spy.mockRejectedValueOnce(new Error("not up"));
    await refreshEngineCommands();
    expect(get(engineCommands)).toHaveLength(1);
    engineCommands.set([]);
    spy.mockRestore();
  });
});

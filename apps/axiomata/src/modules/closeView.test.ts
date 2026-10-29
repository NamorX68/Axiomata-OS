import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

// What a terminal runs in its foreground is the one backend answer these tests set; everything else is the mock's.
const foreground = vi.hoisted(() => ({ answer: null as string | null | Error }));
vi.mock("../core/backend", async (importOriginal) => {
  const real = await importOriginal<typeof import("../core/backend")>();
  return {
    ...real,
    invokeBackend: async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd !== "terminal_foreground") return real.invokeBackend(cmd, args);
      if (foreground.answer instanceof Error) throw foreground.answer;
      return foreground.answer;
    },
  };
});

import { answerChatAsk, ARM_MS, chatAsks } from "../core/chatAsks";
import { registerShellView, setTerminalSession } from "../core/openViews";
import { GUARDED_SUFFIX, invokeAgentAction, manifest, registerShellAction, SHELL_INSTANCE } from "../core/registry";
import { setCloseGuard, setLossProbe, staged } from "../core/staging";
import { instances, loadInstances } from "../core/stores";
import { registerBuiltins } from "./index";

beforeAll(() => registerBuiltins());

const cleanups: (() => void)[] = [];
beforeEach(() => {
  while (cleanups.length) cleanups.pop()?.();
  staged.set([]);
  loadInstances([]);
  chatAsks.set([]);
  foreground.answer = null;
});

function filePanel(id: string, path: string, loss: string | null = null): void {
  staged.update((list) => [...list, { id, type: "file", config: { path } }]);
  setLossProbe(id, () => loss);
}

function terminalTile(id: string, session: string | null): void {
  loadInstances([...get(instances), { id, type: "terminal", x: 0, y: 0, w: 1, h: 1, z: 1, flipped: false, config: {} }]);
  setTerminalSession(id, session);
  cleanups.push(() => setTerminalSession(id, null));
}

const close = (params: Record<string, unknown>) => invokeAgentAction(SHELL_INSTANCE, "close_view", params, "chat");
const stagedIds = () => get(staged).map((panel) => panel.id);
const yes = async () => answerChatAsk(get(chatAsks)[0].id, true, Date.now() + ARM_MS);

describe("closing a view from the chat (CH5)", () => {
  it("is offered as acting at once only when nothing would be lost", () => {
    const shell = manifest().find((entry) => entry.instance_id === SHELL_INSTANCE)?.actions ?? [];
    expect(shell.find((action) => action.name === "close_view")?.description).toContain(GUARDED_SUFFIX);
    expect(shell.find((action) => action.name === "open_views")?.description).not.toContain(GUARDED_SUFFIX);
  });

  it("lists what is open and what closing each would lose", async () => {
    filePanel("p1", "Notes/a.md", "unsaved changes to Notes/a.md");
    terminalTile("t1", "s1");
    foreground.answer = "cargo";
    cleanups.push(registerShellView("handbook", { title: "Handbook — Kanban", close: () => {} }));
    const views = (await invokeAgentAction(SHELL_INSTANCE, "open_views", {})) as Record<string, unknown>[];
    expect(views).toEqual([
      { view: "file", id: "p1", title: "Notes/a.md", path: "Notes/a.md", wouldLose: "unsaved changes to Notes/a.md" },
      expect.objectContaining({ view: "terminal", id: "t1", wouldLose: expect.stringContaining("“cargo”") }),
      { view: "handbook", id: "handbook", title: "Handbook — Kanban", path: null, wouldLose: null },
    ]);
  });

  it("closes at once what loses nothing, through the view's own close", async () => {
    const closed = vi.fn();
    cleanups.push(registerShellView("settings", { title: "Settings", close: closed }));
    expect(await close({ view: "settings" })).toEqual({ closed: "settings", id: "settings" });
    expect(closed).toHaveBeenCalledOnce();

    filePanel("p1", "Notes/a.md");
    const guard = vi.fn(async () => true);
    setCloseGuard("p1", guard);
    await close({ view: "file" });
    expect(guard, "the panel's own guard is still asked").toHaveBeenCalledOnce();
    expect(stagedIds()).toEqual([]);
    expect(get(chatAsks)).toEqual([]);
  });

  it("asks before unsaved text is lost, says what, and closes only through the panel's guard on yes", async () => {
    filePanel("p1", "Notes/a.md", "unsaved changes to Notes/a.md");
    let letGo = false;
    const guard = vi.fn(async () => letGo);
    setCloseGuard("p1", guard);

    expect(await close({ view: "file" })).toMatchObject({ asked: true, question: "Close Notes/a.md" });
    expect(stagedIds(), "nothing closed before the yes").toEqual(["p1"]);
    expect(guard).not.toHaveBeenCalled();
    const [ask] = get(chatAsks);
    expect(ask.detail).toContain("unsaved changes to Notes/a.md");
    expect(ask.detail).toContain("save or discard");

    // The owner says yes, then keeps the text at the panel's own question: it stays open.
    await yes();
    expect(guard).toHaveBeenCalledOnce();
    expect(stagedIds()).toEqual(["p1"]);

    letGo = true;
    await close({ view: "file", id: "p1" });
    await yes();
    expect(stagedIds()).toEqual([]);
  });

  it("asks before a terminal that runs a program, and closes an idle one at once", async () => {
    terminalTile("busy", "s1");
    foreground.answer = "cargo";
    expect(await close({ view: "terminal", id: "busy" })).toMatchObject({ asked: true });
    expect(get(chatAsks)[0].detail).toContain("“cargo”");
    expect(get(instances).map((inst) => inst.id)).toEqual(["busy"]);
    await yes();
    expect(get(instances)).toEqual([]);

    terminalTile("idle", "s2");
    foreground.answer = null;
    await close({ view: "terminal" });
    expect(get(instances)).toEqual([]);

    // A shell that already ended runs nothing.
    terminalTile("ended", null);
    await close({ view: "terminal" });
    expect(get(instances)).toEqual([]);
  });

  it("asks when it cannot tell what a terminal runs", async () => {
    terminalTile("t1", "s1");
    foreground.answer = new Error("unknown terminal session: s1");
    expect(await close({ view: "terminal" })).toMatchObject({ asked: true });
    expect(get(chatAsks)[0].detail).toContain("could not be checked");
    expect(get(instances)).toHaveLength(1);
  });

  it("closes only the view the owner was shown", async () => {
    filePanel("p1", "Notes/a.md", "unsaved changes to Notes/a.md");
    setCloseGuard("p1", async () => true);
    await close({ view: "file" });
    // Another file opens before the yes: the yes still closes the one that was named.
    filePanel("p2", "Notes/b.md");
    await yes();
    expect(stagedIds()).toEqual(["p2"]);
  });

  it("needs a name among several, and never closes the Studio", async () => {
    filePanel("p1", "Notes/a.md");
    filePanel("p2", "Notes/b.md");
    await expect(close({ view: "file" })).rejects.toThrow(/name one by id/);
    await close({ view: "file", path: "Notes/b.md" });
    expect(stagedIds()).toEqual(["p1"]);
    await expect(close({ view: "file", id: "p9" })).rejects.toThrow(/no open file "p9"/);
    await expect(close({ view: "studio" })).rejects.toThrow(/Studio and its agents are the owner's/);
    await expect(close({ view: "handbook" })).rejects.toThrow(/no open handbook/);
  });
});

describe("the guarded mark", () => {
  it("runs a guarded action that finds nothing to ask, and refuses an ask action that finds nothing", async () => {
    const ran = vi.fn(async () => "ran");
    registerShellAction({
      name: "test_guarded",
      description: "t",
      params: { type: "object", properties: {} },
      chat: "guarded",
      ask: async () => null,
      run: ran,
    });
    registerShellAction({
      name: "test_ask_nothing",
      description: "t",
      params: { type: "object", properties: {} },
      chat: "ask",
      ask: async () => null,
      run: ran,
    });
    expect(await invokeAgentAction(SHELL_INSTANCE, "test_guarded", {})).toBe("ran");
    await expect(invokeAgentAction(SHELL_INSTANCE, "test_ask_nothing", {})).rejects.toThrow(/found nothing to ask/);
    expect(ran).toHaveBeenCalledOnce();
  });
});

describe("a close checked again when it runs", () => {
  it("refuses a close that was not asked about once something would be lost", async () => {
    const { CHAT_ACTIONS } = await import("./chatActions");
    const closeView = CHAT_ACTIONS.find((action) => action.name === "close_view");
    filePanel("p1", "Notes/a.md", "unsaved changes to Notes/a.md");
    // As if the text was typed between the bridge's check and the close: `run` with params nobody was asked about.
    await expect(closeView?.run({ view: "file", id: "p1" }, {} as never)).rejects.toThrow(/would lose unsaved/);
    expect(stagedIds()).toEqual(["p1"]);
  });
});

import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

import { brainView } from "../core/brainView";
import { on } from "../core/bus";
import { chatAsks } from "../core/chatAsks";
import {
  ASKS_SUFFIX,
  getModule,
  invokeAgentAction,
  manifest,
  registerShellAction,
  SHELL_INSTANCE,
} from "../core/registry";
import { FILE_PANEL, staged } from "../core/staging";
import { glassBlur, instances } from "../core/stores";
import { loadInstances } from "../core/stores";
import { CHAT_ACTIONS } from "./chatActions";
import { registerBuiltins } from "./index";

beforeAll(() => registerBuiltins());
beforeEach(() => {
  loadInstances([]);
  chatAsks.set([]);
});

const shellActions = () => manifest().find((entry) => entry.instance_id === SHELL_INSTANCE)?.actions ?? [];

describe("what the chat may operate (CH5)", () => {
  it("offers the views and the display at once, and says which actions ask first", () => {
    const actions = new Map(shellActions().map((action) => [action.name, action.description]));
    for (const name of ["open_view", "set_display", "start_skill", "save_routine", "start_card"]) {
      expect(actions.has(name), name).toBe(true);
    }
    expect(actions.get("open_view")).not.toContain(ASKS_SUFFIX);
    expect(actions.get("start_skill")).toContain(ASKS_SUFFIX);
    expect(actions.get("save_routine")).toContain(ASKS_SUFFIX);
    expect(actions.get("start_card")).toContain(ASKS_SUFFIX);
  });

  it("opens a terminal tile and a file for the owner at once", async () => {
    await invokeAgentAction(SHELL_INSTANCE, "open_view", { view: "terminal" });
    expect(get(instances).filter((inst) => inst.type === "terminal")).toHaveLength(1);
    await expect(invokeAgentAction(SHELL_INSTANCE, "open_view", { view: "file" })).rejects.toThrow(/needs a path/);
    await invokeAgentAction(SHELL_INSTANCE, "open_view", { view: "file", path: "Inbox/idea.md" });
    expect(get(staged).some((panel) => panel.type === FILE_PANEL)).toBe(true);
  });

  it("never offers deleting, and refuses it from the bridge", async () => {
    for (const type of ["routines-board", "calendar", "reminders"]) {
      expect(getModule(type)?.actions?.find((action) => action.name === "delete")?.chat, type).toBe("never");
    }
    loadInstances([{ id: "r1", type: "routines-board", x: 0, y: 0, w: 1, h: 1, z: 1, flipped: false, config: {} }]);
    const routines = manifest().find((entry) => entry.instance_id === "r1");
    expect(routines?.actions.map((action) => action.name)).not.toContain("delete");
    await expect(invokeAgentAction("r1", "delete", { id: 1 })).rejects.toThrow(/owner's to do/);
  });

  it("asks the owner before a skill runs, and checks the skill first", async () => {
    const answer = await invokeAgentAction(SHELL_INSTANCE, "start_skill", { skill: "mail-digest" });
    expect(answer).toEqual({ asked: true, question: "Run the skill mail-digest" });
    expect(get(chatAsks)).toHaveLength(1);
    await expect(invokeAgentAction(SHELL_INSTANCE, "start_skill", { skill: "no-such-skill" })).rejects.toThrow(
      /no skill named/,
    );
    expect(get(chatAsks)).toHaveLength(1);
  });

  it("asks before a routine is made and describes it", async () => {
    const answer = await invokeAgentAction(SHELL_INSTANCE, "save_routine", {
      name: "Morning",
      cron: "0 0 9 * * *",
      skill: "mail-digest",
    });
    expect(answer).toMatchObject({ asked: true });
    expect(get(chatAsks)[0].what).toBe("Create the routine “Morning” (0 0 9 * * *) running the skill mail-digest");
    await expect(
      invokeAgentAction(SHELL_INSTANCE, "save_routine", { name: "x", cron: "0 0 9 * * *" }),
    ).rejects.toThrow(/either a skill or a prompt/);
  });

  it("refuses an action that does not say what an agent may do with it", async () => {
    registerShellAction({
      name: "test_unmarked",
      description: "a mark this build does not know",
      params: { type: "object", properties: {} },
      chat: "maybe" as never,
      run: async () => "ran",
    });
    expect(shellActions().map((action) => action.name)).not.toContain("test_unmarked");
    await expect(invokeAgentAction(SHELL_INSTANCE, "test_unmarked", {})).rejects.toThrow(/owner's to do/);
  });

  it("shows a routine's prompt in full and who asks", async () => {
    await invokeAgentAction(
      SHELL_INSTANCE,
      "save_routine",
      { name: "Nightly", cron: "0 0 2 * * *", prompt: "Summarise\nthe inbox" },
      "chat",
    );
    const [ask] = get(chatAsks);
    expect(ask.what).toBe("Create the routine “Nightly” (0 0 2 * * *) running a prompt (below)");
    expect(ask.detail).toBe("Prompt: Summarise\nthe inbox");
    expect(ask.origin).toBe("chat");
  });

  it("asks before a write into the calendar or the reminders", () => {
    expect(getModule("calendar")?.actions?.find((action) => action.name === "create")?.chat).toBe("ask");
    expect(getModule("reminders")?.actions?.find((action) => action.name === "create")?.chat).toBe("ask");
    expect(getModule("reminders")?.actions?.find((action) => action.name === "complete")?.chat).toBe("ask");
  });

  it("runs a tile's routine switch with what it showed", async () => {
    loadInstances([{ id: "r1", type: "routines-board", x: 0, y: 0, w: 1, h: 1, z: 1, flipped: false, config: {} }]);
    await expect(invokeAgentAction("r1", "setEnabled", { id: "1", on: "yes" })).rejects.toThrow(/integer id/);
  });

  it("opens a view at once, the Second Brain on a tag", async () => {
    const seen = vi.fn();
    const off = on("open-second-brain", seen);
    try {
      await invokeAgentAction(SHELL_INSTANCE, "open_view", { view: "second-brain", tag: "#Rust" });
      expect(seen).toHaveBeenCalledWith({ focus: null, query: "#rust" });
      await expect(
        invokeAgentAction(SHELL_INSTANCE, "open_view", { view: "second-brain", tag: "a b" }),
      ).rejects.toThrow(/no tag/);
      await expect(invokeAgentAction(SHELL_INSTANCE, "open_view", { view: "nowhere" })).rejects.toThrow(/unknown view/);
    } finally {
      off();
    }
    expect(get(chatAsks)).toEqual([]);
  });

  it("changes the display at once, and nothing when a value is wrong", async () => {
    await invokeAgentAction(SHELL_INSTANCE, "set_display", { brainLayout: "hex", glassBlur: 12 });
    expect(get(brainView).layout).toBe("hex");
    expect(get(glassBlur)).toBe(12);
    await expect(
      invokeAgentAction(SHELL_INSTANCE, "set_display", { brainLayout: "map", glassBlur: 999 }),
    ).rejects.toThrow(/glassBlur/);
    expect(get(brainView).layout, "nothing changed").toBe("hex");
  });
});

describe("the Glut chat's operating tool", () => {
  // `agents/app_tools.rs` names the shell actions a Glut chat turn may call; it must name exactly these.
  const SOURCES = import.meta.glob("../../../../crates/axiomata-core/src/agents/app_tools.rs", {
    query: "?raw",
    import: "default",
    eager: true,
  });
  const source = String(Object.values(SOURCES)[0] ?? "");

  it("lists the chat's shell actions", () => {
    const block = /CHAT_SHELL_ACTIONS: \[&str; \d+\] = \[([^\]]*)\]/.exec(source)?.[1] ?? "";
    const listed = [...block.matchAll(/"([^"]+)"/g)].map((match) => match[1]).sort();
    expect(listed).toEqual([...CHAT_ACTIONS.map((action) => action.name), "openFile"].sort());
  });
});

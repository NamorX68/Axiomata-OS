import { afterEach, describe, expect, it } from "vitest";

import { languageSetting } from "../i18n/lang.svelte";
import {
  conversationsPanel,
  costPanel,
  filterRows,
  helpPanel,
  listQuery,
  modelPanel,
  shownRows,
  skillsPanel,
} from "./slashLists";

afterEach(() => languageSetting.set("system"));

const models = [
  {
    id: "deepseek/v4",
    name: "DeepSeek V4",
    context_length: 1_000_000,
    price: { input_per_m: 0.19, output_per_m: 0.75 },
  },
  { id: "glm/5.3", name: "GLM 5.3", context_length: 128_000 },
];
const valid = (id: string) => /^[a-z0-9./-]+$/.test(id);

describe("the line a list follows", () => {
  it("opens once a list command has its space, and filters by what follows", () => {
    expect(listQuery("/model")).toBeNull();
    expect(listQuery("/model ")).toEqual({ command: "model", filter: "" });
    expect(listQuery("/model deep seek")).toEqual({ command: "model", filter: "deep seek" });
    expect(listQuery("/skills run news")).toEqual({ command: "skills", filter: "news" });
    expect(listQuery("/theme dark")).toBeNull();
    expect(listQuery("hello /model x")).toBeNull();
    expect(listQuery("/Model x")).toEqual({ command: "model", filter: "x" });
  });
});

describe("filtering", () => {
  it("keeps the rows that hold every typed word, in any cell and any case", () => {
    const panel = modelPanel(models, { turn: null, configured: "glm/5.3", provider: "open_router" }, valid);
    expect(filterRows(panel.rows, "DEEP").map((r) => r.id)).toEqual(["deepseek/v4"]);
    expect(filterRows(panel.rows, "128k glm").map((r) => r.id)).toEqual(["glm/5.3"]);
    expect(filterRows(panel.rows, "").length).toBe(2);
  });

  it("offers a typed model id the list does not show, and nothing for a malformed one (Q18)", () => {
    const panel = modelPanel(models, { turn: null, configured: "glm/5.3", provider: "open_router" }, valid);
    expect(shownRows(panel, "other/model")).toEqual([
      { id: "typed", cells: ["Use other/model"], line: "/model other/model", run: true },
    ]);
    expect(shownRows(panel, "Not Valid!")).toEqual([]);
    expect(shownRows(skillsPanel([]), "anything")).toEqual([]);
  });

  it("puts the row named exactly first, and still offers a typed id a longer one contains", () => {
    const panel = modelPanel(
      [{ id: "gpt-4o-mini", name: "" }, { id: "gpt-4o", name: "" }],
      { turn: null, configured: "gpt-4o", provider: "open_router" },
      valid,
    );
    expect(shownRows(panel, "gpt-4o").map((r) => r.id)).toEqual(["gpt-4o", "gpt-4o-mini"]);
    expect(shownRows(panel, "gpt-4").map((r) => r.id)).toEqual(["gpt-4o-mini", "gpt-4o", "typed"]);
  });
});

describe("the lists", () => {
  it("mark the model in use and offer going back when this conversation chose another", () => {
    const chosen = modelPanel(models, { turn: "deepseek/v4", configured: "glm/5.3", provider: "open_router" }, valid);
    expect(chosen.rows[0]).toMatchObject({ id: "default", line: "/model default" });
    expect(chosen.rows.find((r) => r.current)?.id).toBe("deepseek/v4");
    expect(chosen.rows[1]?.cells).toEqual(["DeepSeek V4 · deepseek/v4", "$0.19 / $0.75", "1M"]);
  });

  it("ask before a skill runs, put a command into the bar, open a handbook page", () => {
    expect(skillsPanel([{ name: "news-web", description: "news", harness: "glut" }]).rows[0]?.confirm).toBe(
      "Run news-web? ⏎ yes · Esc no",
    );
    const help = helpPanel(
      [{ name: "model", hint: "[model]", description: "the model", level: 1, source: "app" }],
      [{ name: "kanban", title: "Kanban", summary: "the board" }],
    );
    expect(help.rows.find((r) => r.id === "cmd:app:model")).toMatchObject({ line: "/model ", run: false });
    expect(help.rows.find((r) => r.id === "page:kanban")).toMatchObject({ line: "/help kanban", run: true });
  });

  it("speak German when the app does, the names left as they are", () => {
    languageSetting.set("de");
    const list = conversationsPanel([{ id: "s1", title: "Tests", updated_at: "2026-10-20T08:12:00Z", turns: 3 }], "s1");
    expect(list.title).toBe("Frühere Gespräche");
    expect(list.rows[0]?.cells[2]).toBe("3 Schritte");
    expect(list.rows[0]?.current).toBe(true);
    expect(costPanel(2, 0.5, 0).rows.map((r) => r.cells)).toEqual([
      ["Schritte", "2"],
      ["Kosten", "$0,5"],
    ]);
    expect(skillsPanel([{ name: "news-web", description: "x", harness: "glut" }]).rows[0]?.confirm).toBe(
      "news-web starten? ⏎ ja · Esc nein",
    );
  });
});

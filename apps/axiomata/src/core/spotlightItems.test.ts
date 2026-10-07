import { describe, expect, it } from "vitest";

import type { BoardCard, BoardPlan, GraphFile, IdeProject, Routine, SearchHit, Skill } from "./backend";
import { rankItems } from "./spotlight";
import {
  cardItems,
  commandItems,
  fileNameItems,
  moduleItems,
  planItems,
  projectItems,
  routineItems,
  skillItems,
  suggestionItems,
  withContentHits,
} from "./spotlightItems";

describe("spotlight items", () => {
  it("offers the shell's own actions, and a short list of them for an empty query", () => {
    expect(commandItems().map((c) => c.id)).toContain("new-note");
    expect(suggestionItems().map((c) => c.id)).toEqual(["new-note", "kanban", "studio", "brain", "settings"]);
  });

  it("leaves out panel-only modules and singletons that are already placed", () => {
    const modules = [
      { type: "clock", title: "Clock" },
      { type: "file", title: "File", stageOnly: true },
      { type: "terminal", title: "Terminal", singleton: true },
      { type: "mail", title: "Mail", singleton: true },
    ];
    const items = moduleItems(modules, (type) => type === "terminal");
    expect(items.map((i) => i.id)).toEqual(["clock", "mail"]);
  });

  it("makes a skill findable by name and by its description", () => {
    const skills = [{ name: "mail-digest", description: "Summarise the inbox", backend: "opencode" }] as Skill[];
    const rows = rankItems("inbox", { skill: skillItems(skills) });
    expect(rows.map((r) => r.item.id)).toEqual(["mail-digest"]);
    expect(rows[0].item.payload).toEqual({ type: "skill", name: "mail-digest" });
  });

  it("shows whether a routine is on, with its cron, and opens the Routines board", () => {
    const routines = [{ id: 7, name: "Nightly", cron_expr: "0 0 3 * * *", enabled: false }] as Routine[];
    const [routine] = routineItems(routines);
    expect(routine.subtitle).toBe("aus · 0 0 3 * * *");
    expect(routine.payload).toEqual({ type: "routines" });
  });

  it("finds a card by its number and by a label, and names its board", () => {
    const cards = [{ id: 57, board_id: 1, title: "Tools einklappbar", labels: ["ui"] }] as BoardCard[];
    const items = cardItems(cards, new Map([[1, "Axiomata-OS"]]));
    expect(items[0].subtitle).toBe("#57 · Axiomata-OS");
    expect(rankItems("#57", { card: items })).toHaveLength(1);
    expect(rankItems("ui", { card: items })).toHaveLength(1);
  });

  it("sends a plan to the Flow and a project to the Studio on that project", () => {
    const [plan] = planItems([{ id: 6, name: "Flow-Test", goal: "README", status: "closed" }] as BoardPlan[]);
    expect(plan.payload).toEqual({ type: "studio", mode: "flow" });
    const [project] = projectItems([{ id: 2, name: "OChaT", repo_root: "/x/OChaT" }] as IdeProject[]);
    expect(project.payload).toEqual({ type: "studio", projectId: 2 });
  });

  it("finds a file by its name, wherever it lies, and shows the folder", () => {
    const files = [{ path: "Ideen/ToDo.md", title: "Aufgaben", bytes: 1 }] as GraphFile[];
    const names = fileNameItems(files);
    expect(names[0].subtitle).toBe("Ideen");
    expect(rankItems("ToDo.md", { file: names }).map((r) => r.item.id)).toEqual(["Ideen/ToDo.md"]);
    expect(rankItems("aufgaben", { file: names })).toHaveLength(1);
  });

  it("lays the content hits over the names: a known file gets its count, an unknown one is added with its line", () => {
    const names = fileNameItems([{ path: "a/notes.md", title: "notes", bytes: 1 }] as GraphFile[]);
    const hits = [
      { path: "a/notes.md", line: 3, snippet: "budget here", matches: 4 },
      { path: "b/other.md", line: 1, snippet: "  the budget  ", matches: 2 },
    ] as SearchHit[];
    const merged = withContentHits(names, hits);
    expect(merged).toHaveLength(2);
    expect(merged[0].contentMatches).toBe(4);
    expect(merged[1]).toMatchObject({ id: "b/other.md", title: "other.md", subtitle: "b", detail: "the budget", contentMatches: 2 });
    expect(rankItems("budget", { file: merged }).map((r) => r.item.id)).toEqual(["a/notes.md", "b/other.md"]);
  });
});

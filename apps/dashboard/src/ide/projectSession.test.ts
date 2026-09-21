import { get } from "svelte/store";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { IdeProject } from "../core/backend";
import { toast } from "../core/toast";
import { allTabs, findTab, singleGroupLayout, type Layout } from "./layout";
import * as session from "./projectSession";
import * as agentApi from "./agents";
import * as projects from "./projects";

vi.mock("./projects", () => ({
  listProjects: vi.fn(),
  openProject: vi.fn(),
  createProject: vi.fn(),
  setProjectRoot: vi.fn(),
  deleteProject: vi.fn(),
  flushLayout: vi.fn(async () => {}),
  saveLayoutSoon: vi.fn(),
  cancelLayoutWrite: vi.fn(),
}));
vi.mock("./agents", () => ({
  listAgents: vi.fn(async () => []),
  createAgent: vi.fn(),
  updateAgent: vi.fn(),
  deleteAgent: vi.fn(),
}));
vi.mock("../core/toast", () => ({ toast: vi.fn() }));

const api = vi.mocked(projects);
const agents = vi.mocked(agentApi);
const toasted = vi.mocked(toast);

function project(id: number, over: Partial<IdeProject> = {}): IdeProject {
  return {
    id,
    name: `project-${id}`,
    repo_root: `/repo/${id}`,
    layout_json: null,
    created_at: "2026-09-01T00:00:00Z",
    last_opened_at: null,
    root_exists: true,
    ...over,
  };
}

/** Resolves only when `release()` is called — for overlapping opens. */
function deferred<T>() {
  let release!: (value: T) => void;
  const promise = new Promise<T>((resolve) => (release = resolve));
  return { promise, release };
}

beforeEach(() => {
  vi.clearAllMocks();
  session.resetSessionForTests();
  api.listProjects.mockResolvedValue([]);
  api.flushLayout.mockResolvedValue(undefined);
  agents.listAgents.mockResolvedValue([]);
});

afterEach(() => {
  session.resetSessionForTests();
});

describe("open", () => {
  it("writes the outgoing layout before asking for the next project", async () => {
    const order: string[] = [];
    api.flushLayout.mockImplementation(async () => void order.push("flush"));
    api.openProject.mockImplementation(async () => (order.push("open"), project(1)));

    await session.open(1);
    expect(order).toEqual(["flush", "open"]);
  });

  it("returns a starting layout for a project that has none", async () => {
    api.openProject.mockResolvedValue(project(1));
    const layout = await session.open(1);
    expect(allTabs(layout!).map((t) => t.kind)).toEqual(["terminal"]);
    expect(get(session.session).current?.id).toBe(1);
  });

  it("points the panes of a stored layout at the project's current folder", async () => {
    const stored = JSON.stringify({
      version: 1,
      root: {
        type: "tabs",
        id: "g1",
        active: "t1",
        tabs: [{ id: "t1", kind: "terminal", title: "Terminal", config: { cwd: "/where/it/used/to/be" } }],
      },
    });
    api.openProject.mockResolvedValue(project(1, { layout_json: stored, repo_root: "/repo/moved" }));

    const layout = await session.open(1);
    expect(findTab(layout!, "t1")!.tab.config).toEqual({ cwd: "/repo/moved" });
  });

  it("says so when a stored layout cannot be read, instead of losing it quietly", async () => {
    api.openProject.mockResolvedValue(project(1, { layout_json: "{ this is not a layout" }));
    const layout = await session.open(1);

    expect(allTabs(layout!).map((t) => t.kind)).toEqual(["terminal"]);
    expect(toasted).toHaveBeenCalledWith(expect.stringContaining("could not be read"), "warning");
  });

  it("discards an answer that a newer open has overtaken", async () => {
    // The double-click case: two round trips in flight, the first one slower.
    const slow = deferred<IdeProject | null>();
    api.openProject.mockImplementationOnce(() => slow.promise).mockResolvedValueOnce(project(2));

    const first = session.open(1);
    const second = await session.open(2);
    slow.release(project(1));

    expect(await first).toBeNull();
    expect(second).not.toBeNull();
    expect(get(session.session).current?.id).toBe(2);
  });

  it("reports a failure and leaves the view alone", async () => {
    api.openProject.mockRejectedValue(new Error("database is locked"));
    expect(await session.open(1)).toBeNull();
    expect(toasted).toHaveBeenCalledWith("database is locked", "danger");
  });

  it("refreshes the list when the project turned out to be gone", async () => {
    api.openProject.mockResolvedValue(null);
    expect(await session.open(9)).toBeNull();
    expect(api.listProjects).toHaveBeenCalled();
    expect(get(session.session).current).toBeNull();
  });
});

describe("start", () => {
  it("opens the most recently used project, which the store sorts first", async () => {
    api.listProjects.mockResolvedValue([project(3), project(1)]);
    api.openProject.mockResolvedValue(project(3));

    expect(await session.start()).not.toBeNull();
    expect(api.openProject).toHaveBeenCalledWith(3);
  });

  it("opens nothing when there are no projects", async () => {
    expect(await session.start()).toBeNull();
    expect(api.openProject).not.toHaveBeenCalled();
  });
});

describe("changeRoot", () => {
  const layout: Layout = singleGroupLayout([
    { id: "t1", kind: "terminal", title: "Terminal", config: { cwd: "/repo/1" } },
  ]);

  it("corrects the open project's panes to the new folder", async () => {
    api.openProject.mockResolvedValue(project(1));
    await session.open(1);
    api.setProjectRoot.mockResolvedValue(project(1, { repo_root: "/repo/elsewhere" }));

    const next = await session.changeRoot(1, "/repo/elsewhere", layout);
    expect(findTab(next!, "t1")!.tab.config).toEqual({ cwd: "/repo/elsewhere" });
    expect(get(session.session).current?.repo_root).toBe("/repo/elsewhere");
  });

  it("leaves the layout alone when the project moved is not the open one", async () => {
    api.setProjectRoot.mockResolvedValue(project(7, { repo_root: "/somewhere" }));
    expect(await session.changeRoot(7, "/somewhere", layout)).toBeNull();
  });
});

describe("remove", () => {
  it("drops a queued write for the project it is deleting", async () => {
    api.openProject.mockResolvedValue(project(1));
    await session.open(1);
    api.deleteProject.mockResolvedValue(true);

    expect(await session.remove(1)).toBe(true);
    expect(api.cancelLayoutWrite).toHaveBeenCalledWith(1);
    expect(get(session.session).current).toBeNull();
  });

  it("reports that the open project was not the one removed", async () => {
    api.openProject.mockResolvedValue(project(1));
    await session.open(1);
    api.deleteProject.mockResolvedValue(true);

    expect(await session.remove(2)).toBe(false);
    expect(get(session.session).current?.id).toBe(1);
  });
});

describe("agents", () => {
  const profile = {
    name: "Builder",
    harness: "opencode" as const,
    command: "",
    model: null,
    env: "",
  };

  function row(id: number, name = "Builder") {
    return {
      id,
      project_id: 1,
      name,
      harness: "opencode" as const,
      command: "",
      model: null,
      env: "",
      created_at: "2026-09-01T00:00:00Z",
      updated_at: "2026-09-01T00:00:00Z",
      worktree_path: null,
      branch: null,
      port: null,
      effective_command: "opencode",
      effective_env: `AXIOMATA_AGENT_ID=${id}\nAXIOMATA_AGENT_NAME=${name}`,
    };
  }

  async function withOpenProject() {
    api.openProject.mockResolvedValue(project(1));
    agents.listAgents.mockResolvedValue([row(1)]);
    await session.open(1);
  }

  it("loads the open project's agents with it", async () => {
    await withOpenProject();
    expect(get(session.session).agents.map((a) => a.name)).toEqual(["Builder"]);
    expect(agents.listAgents).toHaveBeenCalledWith(1);
  });

  it("adds an agent to the open project and nothing without one", async () => {
    expect(await session.addAgent(profile)).toBeNull();
    expect(agents.createAgent).not.toHaveBeenCalled();

    await withOpenProject();
    agents.createAgent.mockResolvedValue(row(2, "Reviewer"));
    agents.listAgents.mockResolvedValue([row(1), row(2, "Reviewer")]);

    expect((await session.addAgent(profile))?.name).toBe("Reviewer");
    expect(get(session.session).agents).toHaveLength(2);
  });

  it("reports a refused name instead of throwing at the view", async () => {
    await withOpenProject();
    agents.createAgent.mockRejectedValue(new Error('this project already has an agent called "Builder"'));

    expect(await session.addAgent(profile)).toBeNull();
    expect(toasted).toHaveBeenCalledWith(expect.stringContaining("already has an agent"), "danger");
  });

  it("finds an agent by id, which is all a pane stores", async () => {
    await withOpenProject();
    expect(session.agentById(1)?.name).toBe("Builder");
    expect(session.agentById(99)).toBeNull();
  });

  it("forgets the agents when the project holding them is removed", async () => {
    await withOpenProject();
    api.deleteProject.mockResolvedValue(true);

    await session.remove(1);
    expect(get(session.session).agents).toEqual([]);
  });
});

describe("save", () => {
  it("queues a write for the open project and does nothing without one", async () => {
    const layout = singleGroupLayout([{ id: "t1", kind: "terminal", title: "Terminal" }]);
    session.save(layout);
    expect(api.saveLayoutSoon).not.toHaveBeenCalled();

    api.openProject.mockResolvedValue(project(4));
    await session.open(4);
    session.save(layout);
    expect(api.saveLayoutSoon).toHaveBeenCalledWith(4, expect.stringContaining('"version"'));
  });
});

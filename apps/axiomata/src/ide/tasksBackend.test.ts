import { describe, expect, it } from "vitest";

import { describeTask, groupTasks, type TaskInfo } from "./tasksBackend";
import { taskIdOf, taskTab } from "./paneKinds";

const task = (over: Partial<TaskInfo>): TaskInfo => ({
  id: "detected:x",
  label: "x",
  command: "x",
  cwd: null,
  env: [],
  source: "detected",
  group: "other",
  ...over,
});

describe("tasks (frontend)", () => {
  it("groups tasks in the order run, build, test, check, other and leaves empty groups out", () => {
    const groups = groupTasks([task({ id: "1", group: "test" }), task({ id: "2", group: "run" }), task({ id: "3", group: "test" })]);
    expect(groups.map((g) => [g.group, g.tasks.length])).toEqual([
      ["run", 1],
      ["test", 2],
    ]);
  });

  it("describes a task with its folder and environment, as the confirmation shows it", () => {
    expect(describeTask(task({ command: "mkdocs build", cwd: "site", env: [["LANG", "C"]] }))).toBe("in site: LANG=C mkdocs build");
    expect(describeTask(task({ command: "cargo test" }))).toBe("cargo test");
  });

  it("a task tab names the task and never carries a command", () => {
    const tab = taskTab("cargo test", "detected:cargo-test");
    expect(taskIdOf(tab)).toBe("detected:cargo-test");
    expect(JSON.stringify(tab)).not.toContain("--");
    expect(Object.keys(tab.config ?? {})).toEqual(["taskId"]);
  });
});

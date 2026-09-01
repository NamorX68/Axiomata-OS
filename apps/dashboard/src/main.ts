import { invoke } from "@tauri-apps/api/core";

/** Skill metadata as returned by the `list_skills` command. */
interface Skill {
  name: string;
  description: string;
  backend: string;
}

/** One row of run history as returned by `list_runs` / `run_skill`. */
interface RunRecord {
  id: number | null;
  skill_name: string;
  backend: string;
  status: "success" | "failed";
  exit_code: number | null;
  duration_ms: number;
  error: string | null;
  started_at: string;
}

const RUN_LIMIT = 25;
const POLL_MS = 3000;

/** Names of skills whose Run button is currently disabled (a run in flight). */
const running = new Set<string>();

function el<T extends HTMLElement>(id: string): T {
  const node = document.getElementById(id);
  if (!node) throw new Error(`missing element #${id}`);
  return node as T;
}

async function refreshSkills(): Promise<void> {
  const status = el<HTMLParagraphElement>("skills-status");
  const table = el<HTMLTableElement>("skills-table");
  const body = el<HTMLTableSectionElement>("skills-body");
  try {
    const skills = await invoke<Skill[]>("list_skills");
    body.replaceChildren();
    if (skills.length === 0) {
      status.textContent = "No skills found.";
      table.hidden = true;
      return;
    }
    for (const skill of skills) {
      body.appendChild(skillRow(skill));
    }
    status.textContent = "";
    table.hidden = false;
  } catch (err) {
    status.textContent = `Failed to load skills: ${String(err)}`;
    table.hidden = true;
  }
}

function skillRow(skill: Skill): HTMLTableRowElement {
  const row = document.createElement("tr");
  row.appendChild(cell(skill.name));
  row.appendChild(cell(skill.backend));
  row.appendChild(cell(skill.description));

  const actionCell = document.createElement("td");
  const button = document.createElement("button");
  button.textContent = running.has(skill.name) ? "Running…" : "Run";
  button.disabled = running.has(skill.name);
  button.addEventListener("click", () => runSkill(skill.name));
  actionCell.appendChild(button);
  row.appendChild(actionCell);
  return row;
}

async function runSkill(name: string): Promise<void> {
  running.add(name);
  await refreshSkills();
  try {
    await invoke<RunRecord>("run_skill", { name });
  } catch (err) {
    // A hard failure (skill not found) — surface it in the runs status line.
    el<HTMLParagraphElement>("runs-status").textContent =
      `run_skill(${name}) failed: ${String(err)}`;
  } finally {
    running.delete(name);
    await Promise.all([refreshSkills(), refreshRuns()]);
  }
}

async function refreshRuns(): Promise<void> {
  const status = el<HTMLParagraphElement>("runs-status");
  const table = el<HTMLTableElement>("runs-table");
  const body = el<HTMLTableSectionElement>("runs-body");
  try {
    const runs = await invoke<RunRecord[]>("list_runs", { limit: RUN_LIMIT });
    body.replaceChildren();
    if (runs.length === 0) {
      status.textContent = "No runs recorded yet.";
      table.hidden = true;
      return;
    }
    for (const run of runs) {
      body.appendChild(runRow(run));
    }
    status.textContent = "";
    table.hidden = false;
  } catch (err) {
    status.textContent = `Failed to load runs: ${String(err)}`;
    table.hidden = true;
  }
}

function runRow(run: RunRecord): HTMLTableRowElement {
  const row = document.createElement("tr");
  row.className = run.status === "failed" ? "failed" : "success";
  row.appendChild(cell(run.id === null ? "—" : String(run.id)));
  row.appendChild(cell(run.started_at));
  row.appendChild(cell(run.status));
  row.appendChild(cell(run.skill_name));
  row.appendChild(cell(run.backend));
  row.appendChild(cell(String(run.duration_ms)));
  if (run.error) row.title = run.error;
  return row;
}

function cell(text: string): HTMLTableCellElement {
  const td = document.createElement("td");
  td.textContent = text;
  return td;
}

window.addEventListener("DOMContentLoaded", () => {
  void refreshSkills();
  void refreshRuns();
  setInterval(() => {
    void refreshSkills();
    void refreshRuns();
  }, POLL_MS);
});

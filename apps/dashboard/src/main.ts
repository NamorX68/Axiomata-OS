import { invoke } from "@tauri-apps/api/core";

/** Skill metadata as returned by the `list_skills` command. */
interface Skill {
  name: string;
  description: string;
  backend: string;
}

/** One row of run history, as returned by `list_runs` (a `RunSummary`). The
 *  `run_skill` command returns the fuller `RunRecord`, but this UI only reads
 *  these fields, so the slim shape covers both. */
interface RunSummary {
  id: number;
  skill_name: string;
  backend: string;
  status: "success" | "failed";
  exit_code: number | null;
  duration_ms: number;
  error: string | null;
  started_at: string;
}

/** Memory-router freshness, as returned by `get_memory_status`. */
interface MemoryStatus {
  workspace_root: string;
  last_sync: string | null;
  stale: boolean;
  tracked_files: number;
}

/** What a `sync_memory` call did. */
interface SyncReport {
  written: string[];
  unchanged: number;
  tracked_files: number;
}

const RUN_LIMIT = 25;
const POLL_MS = 3000;

/** True while a "Sync now" call is in flight. */
let memorySyncing = false;

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
    await invoke<RunSummary>("run_skill", { name });
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
    const runs = await invoke<RunSummary[]>("list_runs", { limit: RUN_LIMIT });
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

function runRow(run: RunSummary): HTMLTableRowElement {
  const row = document.createElement("tr");
  row.className = run.status === "failed" ? "failed" : "success";
  row.appendChild(cell(String(run.id)));
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

async function refreshMemory(): Promise<void> {
  const status = el<HTMLParagraphElement>("memory-status");
  const detail = el<HTMLElement>("memory-detail");
  try {
    const m = await invoke<MemoryStatus>("get_memory_status");
    el<HTMLElement>("memory-workspace").textContent = m.workspace_root;
    el<HTMLElement>("memory-tracked").textContent = String(m.tracked_files);
    el<HTMLElement>("memory-last-sync").textContent = m.last_sync ?? "never";
    const badge = el<HTMLSpanElement>("memory-badge");
    badge.textContent = m.stale ? "stale" : "fresh";
    badge.className = `badge ${m.stale ? "stale" : "fresh"}`;
    status.textContent = "";
    detail.hidden = false;
  } catch (err) {
    status.textContent = `Failed to load memory status: ${String(err)}`;
    detail.hidden = true;
  }
}

async function syncMemory(): Promise<void> {
  if (memorySyncing) return;
  memorySyncing = true;
  const button = el<HTMLButtonElement>("memory-sync-btn");
  const result = el<HTMLParagraphElement>("memory-sync-result");
  button.disabled = true;
  button.textContent = "Syncing…";
  try {
    const r = await invoke<SyncReport>("sync_memory");
    result.textContent =
      r.written.length === 0
        ? `Already in sync (${r.tracked_files} tracked files).`
        : `Wrote ${r.written.length} CLAUDE.md file(s); ${r.unchanged} unchanged.`;
  } catch (err) {
    result.textContent = `Sync failed: ${String(err)}`;
  } finally {
    memorySyncing = false;
    button.disabled = false;
    button.textContent = "Sync now";
    await refreshMemory();
  }
}

window.addEventListener("DOMContentLoaded", () => {
  el<HTMLButtonElement>("memory-sync-btn").addEventListener("click", () => {
    void syncMemory();
  });
  void refreshMemory();
  void refreshSkills();
  void refreshRuns();
  setInterval(() => {
    void refreshMemory();
    void refreshSkills();
    void refreshRuns();
  }, POLL_MS);
});

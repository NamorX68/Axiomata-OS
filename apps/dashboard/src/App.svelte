<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { invoke } from "@tauri-apps/api/core";

  import { addInstance } from "./core/stores";
  import { invokeAction, manifest } from "./core/registry";

  /* ---- shapes mirrored from the Rust command layer ---- */

  interface Skill {
    name: string;
    description: string;
    backend: string;
  }

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

  interface MemoryStatus {
    workspace_root: string;
    last_sync: string | null;
    stale: boolean;
    tracked_files: number;
  }

  interface SyncReport {
    written: string[];
    unchanged: number;
    failed: [string, string][];
    tracked_files: number;
  }

  type RoutineTarget =
    | { type: "skill"; value: string }
    | { type: "prompt"; value: string };

  interface Routine {
    id: number;
    name: string;
    cron_expr: string;
    target: RoutineTarget;
    backend: string | null;
    enabled: boolean;
    next_fire_at: string | null;
    last_fired_at: string | null;
  }

  const RUN_LIMIT = 25;
  const POLL_MS = 3000;

  /* ---- state ---- */

  let memory = $state<MemoryStatus | null>(null);
  let memoryError = $state("");
  let memorySyncing = $state(false);
  let memorySyncResult = $state("");

  let skills = $state<Skill[]>([]);
  let skillsError = $state("");
  const running = new SvelteSet<string>();

  let routines = $state<Routine[]>([]);
  let routinesError = $state("");
  const routineToggling = new SvelteSet<number>();
  let routineAdding = $state(false);
  let routineFormResult = $state("");

  let runs = $state<RunSummary[]>([]);
  let runsError = $state("");

  /* add-routine form */
  let formName = $state("");
  let formCron = $state("");
  let formTargetType = $state<"skill" | "prompt">("skill");
  let formTargetValue = $state("");
  let formBackend = $state("");

  /* ---- backend calls ---- */

  async function refreshMemory() {
    try {
      memory = await invoke<MemoryStatus>("get_memory_status");
      memoryError = "";
    } catch (err) {
      memoryError = `Failed to load memory status: ${String(err)}`;
    }
  }

  async function syncMemory() {
    if (memorySyncing) return;
    memorySyncing = true;
    try {
      const r = await invoke<SyncReport>("sync_memory");
      const base =
        r.written.length === 0
          ? `Already in sync (${r.tracked_files} tracked files).`
          : `Wrote ${r.written.length} CLAUDE.md file(s); ${r.unchanged} unchanged.`;
      memorySyncResult =
        r.failed.length === 0
          ? base
          : `${base} ${r.failed.length} failed: ${r.failed
              .map(([p, why]) => `${p} (${why})`)
              .join("; ")}`;
    } catch (err) {
      memorySyncResult = `Sync failed: ${String(err)}`;
    } finally {
      memorySyncing = false;
      await refreshMemory();
    }
  }

  async function refreshSkills() {
    try {
      skills = await invoke<Skill[]>("list_skills");
      skillsError = "";
    } catch (err) {
      skillsError = `Failed to load skills: ${String(err)}`;
    }
  }

  async function runSkill(name: string) {
    running.add(name);
    try {
      await invoke<RunSummary>("run_skill", { name });
    } catch (err) {
      runsError = `run_skill(${name}) failed: ${String(err)}`;
    } finally {
      running.delete(name);
      await refreshRuns();
    }
  }

  async function refreshRuns() {
    try {
      runs = await invoke<RunSummary[]>("list_runs", { limit: RUN_LIMIT });
      runsError = "";
    } catch (err) {
      runsError = `Failed to load runs: ${String(err)}`;
    }
  }

  async function refreshRoutines() {
    try {
      routines = await invoke<Routine[]>("list_routines");
      routinesError = "";
    } catch (err) {
      routinesError = `Failed to load routines: ${String(err)}`;
    }
  }

  async function toggleRoutine(id: number, enabled: boolean) {
    routineToggling.add(id);
    try {
      await invoke<boolean>("set_routine_enabled", { id, enabled });
    } catch (err) {
      routinesError = `set_routine_enabled(#${id}) failed: ${String(err)}`;
    } finally {
      routineToggling.delete(id);
      await refreshRoutines();
    }
  }

  async function addRoutine(event: SubmitEvent) {
    event.preventDefault();
    if (routineAdding) return;
    routineAdding = true;
    const newRoutine = {
      name: formName.trim(),
      cron_expr: formCron.trim(),
      target: { type: formTargetType, value: formTargetValue.trim() },
      backend: formBackend === "" ? null : formBackend,
      enabled: true,
    };
    try {
      const created = await invoke<Routine>("add_routine", { new: newRoutine });
      routineFormResult = `Created routine #${created.id}; next fire ${
        created.next_fire_at ?? "never"
      }.`;
      formName = "";
      formCron = "";
      formTargetValue = "";
      formBackend = "";
    } catch (err) {
      routineFormResult = `Add failed: ${String(err)}`;
    } finally {
      routineAdding = false;
      await refreshRoutines();
    }
  }

  function refreshAll() {
    void refreshMemory();
    void refreshSkills();
    void refreshRoutines();
    void refreshRuns();
  }

  // Step-2 scaffolding: prove the registry/store/context pipeline end to end.
  // Removed once the module picker (step 7) exists.
  async function devProbe() {
    const inst = addInstance({
      type: "dummy",
      x: 40,
      y: 40,
      w: 260,
      h: 160,
      config: {},
    });
    console.log("registry.manifest() →", manifest());
    console.log("invokeAction(ping) →", await invokeAction(inst.id, "ping", {}));
  }

  onMount(() => {
    refreshAll();
    const id = setInterval(refreshAll, POLL_MS);
    return () => clearInterval(id);
  });
</script>

<main class="ax-placeholder">
  <header>
    <h1>Axiomata-OS</h1>
    <p class="hint">
      Placeholder shell — module canvas (M5) is being built on this branch.
      Theming now runs through <code>--ax-*</code> tokens; the four panels below
      become canvas modules in later steps.
    </p>
    {#if import.meta.env.DEV}
      <button type="button" onclick={devProbe}>dev: add dummy + log manifest</button>
    {/if}
  </header>

  <section>
    <h2>Memory router</h2>
    {#if memoryError}
      <p class="status">{memoryError}</p>
    {:else if memory}
      <dl>
        <dt>Workspace</dt>
        <dd>{memory.workspace_root}</dd>
        <dt>Tracked files</dt>
        <dd>{memory.tracked_files}</dd>
        <dt>Last sync</dt>
        <dd>{memory.last_sync ?? "never"}</dd>
        <dt>State</dt>
        <dd>
          <span class="badge" class:stale={memory.stale} class:fresh={!memory.stale}>
            {memory.stale ? "stale" : "fresh"}
          </span>
        </dd>
      </dl>
    {:else}
      <p class="status">Loading…</p>
    {/if}
    <button type="button" disabled={memorySyncing} onclick={syncMemory}>
      {memorySyncing ? "Syncing…" : "Sync now"}
    </button>
    {#if memorySyncResult}<p class="status">{memorySyncResult}</p>{/if}
  </section>

  <section>
    <h2>Skills</h2>
    {#if skillsError}
      <p class="status">{skillsError}</p>
    {:else if skills.length === 0}
      <p class="status">No skills found.</p>
    {:else}
      <table>
        <thead>
          <tr><th>Name</th><th>Backend</th><th>Description</th><th></th></tr>
        </thead>
        <tbody>
          {#each skills as skill (skill.name)}
            <tr>
              <td>{skill.name}</td>
              <td>{skill.backend}</td>
              <td>{skill.description}</td>
              <td>
                <button
                  type="button"
                  disabled={running.has(skill.name)}
                  onclick={() => runSkill(skill.name)}
                >
                  {running.has(skill.name) ? "Running…" : "Run"}
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>

  <section>
    <h2>Routines</h2>
    {#if routinesError}
      <p class="status">{routinesError}</p>
    {:else if routines.length === 0}
      <p class="status">No routines defined.</p>
    {:else}
      <table>
        <thead>
          <tr>
            <th>#</th><th>Name</th><th>Schedule</th><th>Target</th>
            <th>Next fire</th><th>Last fired</th><th></th>
          </tr>
        </thead>
        <tbody>
          {#each routines as routine (routine.id)}
            <tr class:success={routine.enabled} class:failed={!routine.enabled}>
              <td>{routine.id}</td>
              <td>{routine.name}</td>
              <td>{routine.cron_expr}</td>
              <td>{routine.target.type}: {routine.target.value}</td>
              <td>{routine.next_fire_at ?? "—"}</td>
              <td>{routine.last_fired_at ?? "never"}</td>
              <td>
                <button
                  type="button"
                  disabled={routineToggling.has(routine.id)}
                  onclick={() => toggleRoutine(routine.id, !routine.enabled)}
                >
                  {routineToggling.has(routine.id)
                    ? "…"
                    : routine.enabled
                      ? "Disable"
                      : "Enable"}
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}

    <form class="routine-form" onsubmit={addRoutine}>
      <input type="text" placeholder="name" bind:value={formName} required />
      <input
        class="cron"
        type="text"
        placeholder="0 */2 * * * *  (sec min hour dom mon dow)"
        bind:value={formCron}
        required
      />
      <select bind:value={formTargetType}>
        <option value="skill">skill</option>
        <option value="prompt">prompt</option>
      </select>
      <input
        type="text"
        placeholder="skill name / prompt text"
        bind:value={formTargetValue}
        required
      />
      <select bind:value={formBackend}>
        <option value="">default backend</option>
        <option value="claude-code">claude-code</option>
        <option value="ollama">ollama</option>
      </select>
      <button type="submit" disabled={routineAdding}>Add routine</button>
    </form>
    {#if routineFormResult}<p class="status">{routineFormResult}</p>{/if}
  </section>

  <section>
    <h2>Recent runs</h2>
    {#if runsError}
      <p class="status">{runsError}</p>
    {:else if runs.length === 0}
      <p class="status">No runs recorded yet.</p>
    {:else}
      <table>
        <thead>
          <tr>
            <th>#</th><th>Started</th><th>Status</th><th>Skill</th>
            <th>Backend</th><th>ms</th>
          </tr>
        </thead>
        <tbody>
          {#each runs as run (run.id)}
            <tr
              class:success={run.status === "success"}
              class:failed={run.status === "failed"}
              title={run.error ?? ""}
            >
              <td>{run.id}</td>
              <td>{run.started_at}</td>
              <td>{run.status}</td>
              <td>{run.skill_name}</td>
              <td>{run.backend}</td>
              <td>{run.duration_ms}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>
</main>

<style>
  .ax-placeholder {
    max-width: 960px;
    margin: 0 auto;
    padding: var(--ax-space-5);
  }

  header h1 {
    font-size: var(--ax-font-size-xl);
    margin-bottom: var(--ax-space-1);
  }

  h2 {
    font-size: var(--ax-font-size-lg);
    margin: var(--ax-space-5) 0 var(--ax-space-2);
  }

  .hint {
    color: var(--ax-text-muted);
    margin-top: 0;
  }
  .hint code {
    font-family: var(--ax-font-mono);
    font-size: 0.9em;
  }

  .status {
    color: var(--ax-text-muted);
    font-style: italic;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    overflow: hidden;
  }

  th,
  td {
    text-align: left;
    padding: var(--ax-space-1) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    vertical-align: top;
  }

  th {
    background: var(--ax-surface-3);
    font-weight: 600;
  }

  tr:last-child td {
    border-bottom: none;
  }

  tr.failed td:nth-child(3) {
    color: var(--ax-danger);
    font-weight: 600;
  }
  tr.success td:nth-child(3) {
    color: var(--ax-success);
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--ax-space-1) var(--ax-space-4);
    margin: var(--ax-space-2) 0;
  }
  dt {
    font-weight: 600;
    color: var(--ax-text-muted);
  }
  dd {
    margin: 0;
    word-break: break-all;
  }

  .routine-form {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-2);
    margin-top: var(--ax-space-3);
  }
  .routine-form .cron {
    flex: 1 1 20rem;
  }

  .badge {
    display: inline-block;
    padding: 2px var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    font-size: 0.85em;
    font-weight: 600;
  }
  .badge.fresh {
    background: color-mix(in srgb, var(--ax-success) 22%, transparent);
    color: var(--ax-success);
  }
  .badge.stale {
    background: color-mix(in srgb, var(--ax-warning) 22%, transparent);
    color: var(--ax-warning);
  }
</style>

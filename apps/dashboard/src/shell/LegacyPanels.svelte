<!--
  The pre-M5 placeholder panels (memory / skills / routines / runs), kept
  verbatim so the app stays useful while the canvas is built. Each <section>
  is deleted as its canvas module lands (steps 8–10); the file goes with the
  last one.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { invokeBackend as invoke } from "../core/backend";
  import type { Routine, RunSummary, Skill } from "../core/backend";

  const RUN_LIMIT = 25;
  const POLL_MS = 3000;

  /* ---- state ---- */

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
    void refreshSkills();
    void refreshRoutines();
    void refreshRuns();
  }

  onMount(() => {
    refreshAll();
    const id = setInterval(refreshAll, POLL_MS);
    return () => clearInterval(id);
  });
</script>

<div class="ax-legacy">

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
</div>

<style>
  .ax-legacy {
    max-width: 960px;
    margin: 0 auto;
    padding: 0 var(--ax-space-5) var(--ax-space-6);
  }

  h2 {
    font-size: var(--ax-font-size-base);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    margin: var(--ax-space-5) 0 var(--ax-space-2);
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


  .routine-form {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-2);
    margin-top: var(--ax-space-3);
  }
  .routine-form .cron {
    flex: 1 1 20rem;
  }

</style>

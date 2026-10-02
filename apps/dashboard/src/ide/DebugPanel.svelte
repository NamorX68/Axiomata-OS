<!--
  The Debug view of the activity rail (#51): pick what to start, then step through it.

  Idle, it lists the configurations — **detected** ones (pytest, a package's `__main__`, `main.py` …), the
  project's own `.axiomata/debug.json` (greyed out behind a "Review" until the owner confirmed those exact
  bytes, as for Run) and "Current file". Running, it shows the toolbar (continue, step over/in/out, stop),
  the call stack, the variables of the selected frame, the breakpoints and a console that runs expressions
  in that frame. The state lives in `ide/debug.ts`; the gutter and the file opened at a stop are the
  editor's.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import { breakpoints, clearBreakpoints } from "./breakpoints";
  import { debug, debugProblem, startDebugging, toggleDebugBreakpoint } from "./debug";
  import {
    isDebuggable,
    listDebugConfigs,
    removeDebugConfig,
    saveDebugConfig,
    trustDebugFile,
    type DebugConfigInfo,
    type DebugListInfo,
  } from "./debugBackend";
  import { absoluteInside } from "./outputPath";
  import { debugKeyAction, KEY_LABEL } from "./debugKeys";
  import { EMPTY_FORM, formOf, toNewConfig, type DebugForm } from "./debugForm";
  import type { DebugVariable } from "./debugBackend";

  let {
    root,
    folder,
    active,
    currentFile,
    onOpen,
    onError,
  }: {
    /** `project:<id>`, or `null` with no project. */
    root: string | null;
    /** The project's folder on disk (to tell which stack frames are the project's own). */
    folder: string | null;
    /** The view is on screen (the list is read again each time it is shown). */
    active: boolean;
    /** The file in front in the dock, offered as "Current file". */
    currentFile: { root: string; rel: string } | null;
    /** Opens a file at a (one-based) line — a breakpoint in the list. */
    onOpen: (rel: string, line: number) => void;
    onError: (message: string) => void;
  } = $props();

  const ds = debug.state;

  let listed = $state<DebugListInfo | null>(null);
  let reviewing = $state(false);
  let choice = $state("");
  let expression = $state("");
  /** Run the program in a terminal pane: for a TUI, or one that reads from the keyboard. */
  let inTerminal = $state(false);
  /** Expanded variable nodes, by `variablesReference`. */
  let open = $state<Record<number, boolean>>({});

  async function refresh(): Promise<void> {
    if (!root) {
      listed = null;
      return;
    }
    try {
      listed = await listDebugConfigs(root);
    } catch (err) {
      listed = null;
      onError((err as { message?: string }).message ?? String(err));
    }
  }

  $effect(() => {
    void root;
    if (active) void refresh();
  });
  onMount(() => void refresh());

  async function allow(): Promise<void> {
    if (!root || !listed?.project_file) return;
    try {
      await trustDebugFile(root, listed.project_file.hash);
      reviewing = false;
    } catch (err) {
      onError((err as { message?: string }).message ?? String(err));
    }
    await refresh();
  }

  // ---- a configuration of your own, kept in the project's .axiomata/debug.json ----

  /** `null` = closed, `""` = a new one, else the name of the one being edited. */
  let editing = $state<string | null>(null);
  let form = $state<DebugForm>({ ...EMPTY_FORM });
  let saving = $state(false);

  function startNew(): void {
    editing = "";
    form = { ...EMPTY_FORM, target: fileRel ?? "" };
  }

  function startEdit(c: DebugConfigInfo): void {
    editing = c.name;
    form = formOf(c);
  }

  async function submit(): Promise<void> {
    if (!root || editing === null || saving) return;
    saving = true;
    try {
      const saved = toNewConfig(form);
      await saveDebugConfig(root, saved, editing === "" ? null : editing);
      editing = null;
      await refresh();
      choice = `named:${saved.name}`;
      picked = true;
    } catch (err) {
      onError((err as { message?: string }).message ?? String(err));
    } finally {
      saving = false;
    }
  }

  async function remove(c: DebugConfigInfo): Promise<void> {
    if (!root || !window.confirm(`Remove “${c.name}” from the project's debug.json?`)) return;
    try {
      await removeDebugConfig(root, c.name);
      await refresh();
    } catch (err) {
      onError((err as { message?: string }).message ?? String(err));
    }
  }

  const chosen = $derived(choice.startsWith("named:") ? listed?.configs.find((c) => c.name === choice.slice(6)) : undefined);

  const fileRel = $derived(currentFile && currentFile.root === root && isDebuggable(currentFile.rel) ? currentFile.rel : null);
  const projectConfigs = $derived(listed?.configs.filter((c) => !c.detected) ?? []);
  const locked = (c: DebugConfigInfo) => !c.detected && !listed?.project_file?.trusted;
  const choices = $derived.by(() => {
    const out: { value: string; label: string; disabled: boolean }[] = [];
    // The project's own ways to start come first (an app is debugged from its entry point, not from
    // whichever file is open); the file in front is the fallback.
    for (const c of listed?.configs ?? []) out.push({ value: `named:${c.name}`, label: c.name, disabled: locked(c) });
    if (fileRel) out.push({ value: "file", label: `Current file — ${fileRel}`, disabled: false });
    return out;
  });
  /** The owner picked something themselves; until then the first way to start the project is the choice. */
  let picked = $state(false);
  // Keep the choice on something that exists — and, until the owner chooses, on the project's own start (the
  // list is read after the panel appears, so “Current file” alone must not stay the choice once it arrives).
  $effect(() => {
    const first = choices.find((c) => !c.disabled)?.value ?? "";
    if (!picked || !choices.some((c) => c.value === choice && !c.disabled)) choice = first;
  });

  const busy = $derived($ds.phase === "starting" || $ds.phase === "running" || $ds.phase === "stopped");
  async function start(): Promise<void> {
    if (!root || !folder || !choice) return;
    const label = choices.find((c) => c.value === choice)?.label ?? choice;
    const target = choice === "file" && fileRel ? ({ kind: "current_file", rel: fileRel } as const) : ({ kind: "named", name: choice.slice(6) } as const);
    open = {};
    await startDebugging(root, folder, target, label, inTerminal);
  }

  function describe(c: DebugConfigInfo): string {
    const what = c.module ? `-m ${c.module}` : (c.program ?? "");
    return [what, ...c.args].join(" ");
  }

  const mine = $derived(root ? ($breakpoints[root] ?? {}) : {});
  const files = $derived(Object.entries(mine).sort(([a], [b]) => a.localeCompare(b)));
  const total = $derived(files.reduce((n, [, lines]) => n + lines.length, 0));

  async function toggleNode(v: DebugVariable): Promise<void> {
    if (v.variables_reference === 0) return;
    open[v.variables_reference] = !open[v.variables_reference];
    if (open[v.variables_reference]) await debug.expand(v.variables_reference);
  }

  async function toggleScope(ref: number, expensive: boolean): Promise<void> {
    open[ref] = !(open[ref] ?? !expensive);
    if (open[ref]) await debug.expand(ref);
  }

  function keys(e: KeyboardEvent): void {
    if ($ds.phase !== "stopped") return;
    const action = debugKeyAction(e);
    if (!action) return;
    e.preventDefault();
    void debug.control(action);
  }

  function leaf(path: string | null): string {
    return path ? path.split(/[\\/]/).pop()! : "—";
  }

  let consoleEl = $state<HTMLElement | null>(null);
  $effect(() => {
    void $ds.output.length;
    if (consoleEl) consoleEl.scrollTop = consoleEl.scrollHeight;
  });
</script>

<svelte:window onkeydown={keys} />

{#snippet nodes(list: DebugVariable[], depth: number)}
  {#each list as v (v.name)}
    <li>
      <button
        type="button"
        class="var"
        style:padding-left="calc({depth} * 12px * var(--ax-ui-scale) + var(--ax-space-2))"
        class:branch={v.variables_reference > 0}
        title={v.type_name ? `${v.name}: ${v.type_name}` : v.name}
        onclick={() => void toggleNode(v)}
      >
        <span class="caret" aria-hidden="true">{v.variables_reference > 0 ? (open[v.variables_reference] ? "▾" : "▸") : ""}</span>
        <span class="vname">{v.name}</span>
        <span class="vvalue">{v.value}</span>
      </button>
      {#if open[v.variables_reference] && $ds.children[v.variables_reference]}
        <ul>{@render nodes($ds.children[v.variables_reference], depth + 1)}</ul>
      {/if}
    </li>
  {/each}
{/snippet}

<div class="panel">
  <div class="head">
    <span class="title">DEBUG</span>
    {#if $ds.phase === "idle" || $ds.phase === "ended"}
      <IconButton icon="refresh-cw" label="Read the configurations again" size="sm" onclick={() => void refresh()} />
    {/if}
  </div>
  <div class="body">
    {#if !root}
      <p class="note">Open a project to debug it.</p>
    {:else}
      {#if listed?.project_file && !listed.project_file.trusted && $ds.phase !== "running" && $ds.phase !== "stopped"}
        <div class="trust" role="alert">
          <p>
            This project has its own <code>.axiomata/debug.json</code>. It starts programs from the repository, so it
            stays off until you have looked at them.
          </p>
          {#if reviewing}
            <ul class="review">
              {#each projectConfigs as c (c.name)}<li><code>{c.name}: {describe(c)}</code></li>{/each}
            </ul>
            <div class="actions">
              <button type="button" class="ax-btn primary" onclick={() => void allow()}>Allow these</button>
              <button type="button" class="ax-btn" onclick={() => (reviewing = false)}>Cancel</button>
            </div>
          {:else}
            <button type="button" class="ax-btn" onclick={() => (reviewing = true)}>Review…</button>
          {/if}
        </div>
      {/if}

      {#if !busy}
        <div class="start">
          <select bind:value={choice} onchange={() => (picked = true)} aria-label="What to debug" disabled={choices.length === 0}>
            {#each choices as c (c.value)}<option value={c.value} disabled={c.disabled}>{c.label}</option>{/each}
          </select>
          <button type="button" class="ax-btn primary go" disabled={!choice || !folder} onclick={() => void start()}>
            <Icon name="bug" size="sm" /> Debug
          </button>
        </div>
        <label class="terminal">
          <input type="checkbox" bind:checked={inTerminal} />
          Run in a terminal <small>for a TUI or a program that reads the keyboard</small>
        </label>
        {#if chosen && !chosen.detected && !locked(chosen)}
          <span class="own">
            <IconButton icon="pencil" label="Edit {chosen.name}" size="sm" onclick={() => startEdit(chosen)} />
            <IconButton icon="trash-2" label="Remove {chosen.name}" size="sm" onclick={() => void remove(chosen)} />
          </span>
        {/if}
        {#if choices.length === 0 && editing === null}
          <p class="note">Nothing to debug found. Say what to start with “New configuration…”.</p>
        {/if}
        {#if editing === null}
          <button type="button" class="add" onclick={startNew}><Icon name="plus" size="sm" /> New configuration…</button>
        {:else}
          <form
            class="config"
            onsubmit={(event) => {
              event.preventDefault();
              void submit();
            }}
          >
            <p class="form-title">{editing === "" ? "New configuration" : "Edit configuration"}</p>
            <input type="text" bind:value={form.name} placeholder="Name, e.g. OChaT app" spellcheck="false" />
            <input
              type="text"
              bind:value={form.target}
              placeholder="File or module, e.g. src/ocht/main.py or ocht.cli"
              spellcheck="false"
            />
            <input type="text" bind:value={form.args} placeholder="Arguments (optional)" spellcheck="false" />
            <input type="text" bind:value={form.cwd} placeholder="Folder inside the project (optional)" spellcheck="false" />
            <p class="hint">Kept in <code>.axiomata/debug.json</code> of this project.</p>
            <div class="actions">
              <button type="submit" class="ax-btn primary" disabled={saving || !form.name.trim() || !form.target.trim()}>Save</button>
              <button type="button" class="ax-btn" onclick={() => (editing = null)}>Cancel</button>
            </div>
          </form>
        {/if}
        {#each listed?.problems ?? [] as problem (problem)}<p class="problem">{problem}</p>{/each}
      {:else}
        <div class="toolbar" role="toolbar" aria-label="Debugger">
          {#if $ds.phase === "stopped"}
            <IconButton icon="play" label="Continue ({KEY_LABEL.continue})" size="sm" onclick={() => void debug.control("continue")} />
          {:else}
            <IconButton icon="pause" label="Pause" size="sm" disabled={$ds.phase !== "running"} onclick={() => void debug.control("pause")} />
          {/if}
          <IconButton icon="step-forward" label="Step over ({KEY_LABEL.next})" size="sm" disabled={$ds.phase !== "stopped"} onclick={() => void debug.control("next")} />
          <IconButton icon="arrow-down-to-line" label="Step into ({KEY_LABEL.step_in})" size="sm" disabled={$ds.phase !== "stopped"} onclick={() => void debug.control("step_in")} />
          <IconButton icon="arrow-up-from-line" label="Step out ({KEY_LABEL.step_out})" size="sm" disabled={$ds.phase !== "stopped"} onclick={() => void debug.control("step_out")} />
          <IconButton icon="square" label="Stop" size="sm" onclick={() => void debug.stop()} />
        </div>
      {/if}

      <p class="status" class:stopped={$ds.phase === "stopped"}>
        {#if $ds.phase === "starting"}Starting {$ds.configName}…
        {:else if $ds.phase === "running"}Running {$ds.configName}
        {:else if $ds.phase === "stopped"}Paused{$ds.stop ? ` — ${$ds.stop.reason}${$ds.stop.text ? `: ${$ds.stop.text}` : ""}` : ""}
        {:else if $ds.phase === "ended"}Ended{$ds.exitCode !== null ? ` — exit code ${$ds.exitCode}` : ""}
        {/if}
      </p>
      {#if $ds.error}<p class="problem">{$ds.error}</p>{/if}
      {#if $debugProblem && $debugProblem !== $ds.error}<p class="problem">{$debugProblem}</p>{/if}

      {#if $ds.phase === "stopped"}
        <h3>Call stack</h3>
        <ul class="frames">
          {#each $ds.frames as f (f.id)}
            <li>
              <button
                type="button"
                class="frame"
                class:on={f.id === $ds.frameId}
                class:outside={!folder || absoluteInside(f.path, folder) === null}
                title={f.path ?? "No source file"}
                onclick={() => void debug.selectFrame(f.id)}
              >
                <span class="fname">{f.name}</span>
                <span class="floc">{leaf(f.path)}:{f.line}</span>
              </button>
            </li>
          {/each}
        </ul>
        <h3>Variables</h3>
        {#each $ds.scopes as s (s.scope.variables_reference)}
          {@const ref = s.scope.variables_reference}
          <button type="button" class="scope" onclick={() => void toggleScope(ref, s.scope.expensive)}>
            <span class="caret" aria-hidden="true">{(open[ref] ?? !s.scope.expensive) ? "▾" : "▸"}</span>{s.scope.name}
          </button>
          {#if (open[ref] ?? !s.scope.expensive) && s.variables}
            <ul>{@render nodes(s.variables, 0)}</ul>
          {/if}
        {/each}
      {/if}

      <h3>
        Breakpoints<span class="n">{total}</span>
        {#if total > 0 && root}
          <IconButton icon="trash-2" label="Remove all breakpoints" size="sm" onclick={() => clearBreakpoints(root)} />
        {/if}
      </h3>
      {#if total === 0}
        <p class="note">Click a line number in a Python or Rust file to set one.</p>
      {:else}
        <ul class="bps">
          {#each files as [rel, lines] (rel)}
            {#each lines as line (line)}
              <li class="bp">
                <button type="button" class="bp-open" onclick={() => onOpen(rel, line)} title={rel}>
                  <span class="dot" aria-hidden="true"></span>{leaf(rel)}:{line}
                </button>
                <IconButton icon="x" label="Remove" size="sm" onclick={() => root && toggleDebugBreakpoint(root, rel, line)} />
              </li>
            {/each}
          {/each}
        </ul>
      {/if}

      {#if $ds.tui}
        <p class="problem">
          This program draws a full-screen interface, which a console cannot show. Stop it and start again with
          “Run in a terminal” ticked — breakpoints and stepping work the same there.
        </p>
      {/if}
      {#if busy || $ds.output.length > 0}
        <h3>Console</h3>
        <div class="console" bind:this={consoleEl}>
          {#each $ds.output as line, i (i)}<span class="out {line.category}">{line.text}</span>{/each}
        </div>
        {#if $ds.phase === "stopped"}
          <form
            onsubmit={(e) => {
              e.preventDefault();
              void debug.evaluate(expression);
              expression = "";
            }}
          >
            <input type="text" bind:value={expression} placeholder="Evaluate in this frame…" spellcheck="false" autocomplete="off" />
          </form>
        {/if}
      {/if}
    {/if}
  </div>
</div>

<style>
  .panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .head {
    display: flex;
    align-items: center;
    padding: var(--ax-space-1) var(--ax-space-3) var(--ax-space-1) var(--ax-space-4);
    border-bottom: 1px solid var(--ax-border);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .title {
    flex: 1;
    letter-spacing: var(--ax-tracking-wide);
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--ax-space-3);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    margin: var(--ax-space-4) 0 var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-weight: 600;
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }

  .n {
    margin-left: auto;
    font-weight: 400;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .start {
    display: flex;
    gap: var(--ax-space-2);
  }

  .go {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
  }

  select,
  input[type="text"] {
    flex: 1;
    min-width: 0;
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-bg);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  select:focus-visible,
  input[type="text"]:focus-visible {
    outline: var(--ax-focus-ring);
  }

  .toolbar {
    display: flex;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1);
    background: var(--ax-surface-2);
    border-radius: var(--ax-radius-md);
  }

  .status {
    margin: var(--ax-space-2) 0 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .status.stopped {
    color: var(--ax-warning);
  }

  .frame,
  .var,
  .scope,
  .bp-open {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-2);
    width: 100%;
    padding: 2px var(--ax-space-2);
    background: none;
    border: none;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    text-align: left;
    cursor: pointer;
  }

  .frame:hover,
  .var:hover,
  .scope:hover,
  .bp-open:hover {
    background: var(--ax-surface-2);
  }

  /* Code outside the project (the standard library, generated files): shown, but not one to open. */
  .frame.outside .fname {
    color: var(--ax-text-muted);
    font-style: italic;
  }

  .frame.on {
    background: var(--ax-surface-3);
  }

  .fname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .floc {
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .scope {
    font-weight: 600;
  }

  .caret {
    flex: 0 0 auto;
    width: 1em;
    color: var(--ax-text-muted);
  }

  .var {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .vname {
    flex: 0 0 auto;
    color: var(--ax-accent);
  }

  .vvalue {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ax-text);
  }

  .bp {
    display: flex;
    align-items: center;
  }

  .dot {
    flex: 0 0 auto;
    width: calc(8px * var(--ax-ui-scale));
    height: calc(8px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-pill);
    background: var(--ax-danger);
  }

  .bp-open {
    align-items: center;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .console {
    max-height: calc(220px * var(--ax-ui-scale));
    overflow-y: auto;
    padding: var(--ax-space-2);
    background: var(--ax-bg);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .out.stderr {
    color: var(--ax-danger);
  }

  .out.repl {
    color: var(--ax-accent);
  }

  .out.console {
    color: var(--ax-text-muted);
  }

  form {
    display: flex;
    margin-top: var(--ax-space-2);
  }

  form.config {
    flex-direction: column;
    gap: var(--ax-space-2);
    padding-top: var(--ax-space-3);
    border-top: 1px solid var(--ax-border);
  }

  .form-title,
  .hint {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .terminal {
    display: block;
    margin-top: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }

  .terminal small {
    display: block;
    margin-left: calc(1.4em);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .own {
    display: flex;
    gap: var(--ax-space-1);
    margin-top: var(--ax-space-1);
  }

  .add {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-2);
    margin-top: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
    background: none;
    border: none;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .add:hover {
    color: var(--ax-text);
  }

  .note,
  .problem {
    margin: var(--ax-space-3) 0 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  .problem {
    color: var(--ax-warning);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .trust {
    padding: var(--ax-space-3);
    margin-bottom: var(--ax-space-3);
    border: 1px solid var(--ax-warning);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-2);
    font-size: var(--ax-font-size-sm);
  }

  .trust p {
    margin: 0 0 var(--ax-space-2);
  }

  .actions {
    display: flex;
    gap: var(--ax-space-2);
  }
</style>

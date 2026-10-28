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
  import { get } from "svelte/store";

  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import SectionHead from "../ui/SectionHead.svelte";
  import Notice from "../ui/Notice.svelte";
  import StatusMark from "../ui/StatusMark.svelte";
  import type { StatusTone } from "../ui/status";
  import BreakpointEditor from "./BreakpointEditor.svelte";
  import { breakpointInfo, breakpoints, clearBreakpoints, infoOf } from "./breakpoints";
  import { debug, debugProblem, editDebugBreakpoint, loadWatches, saveWatches, startDebugging, toggleDebugBreakpoint } from "./debug";
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
  import { EMPTY_FORM, formOf, splitArgs, toNewConfig, type DebugForm } from "./debugForm";
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

  // ---- watch expressions: kept per project, evaluated at every stop ----
  let watchInput = $state("");
  $effect(() => {
    debug.setWatches(root ? loadWatches(root) : []);
  });

  function addWatch(expression: string): void {
    debug.addWatch(expression);
    if (root) saveWatches(root, get(ds).watches.map((w) => w.expr));
  }

  function removeWatch(index: number): void {
    debug.removeWatch(index);
    if (root) saveWatches(root, get(ds).watches.map((w) => w.expr));
  }

  /** The breakpoint whose condition is being edited in the list. */
  let editingBp = $state<{ rel: string; line: number } | null>(null);

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

  /** Arguments typed for this run, per configuration (empty = the configuration's own). */
  let argsByChoice = $state<Record<string, string>>({});
  const argsPlaceholder = $derived.by(() => {
    const own = chosen?.args ?? [];
    return own.length > 0 ? `Arguments — ${own.join(" ")}` : "Arguments (optional)";
  });

  const busy = $derived($ds.phase === "starting" || $ds.phase === "running" || $ds.phase === "stopped");
  /** The session's mark: alive while it runs, waiting on the owner while paused, still once it ended. */
  const sessionTone = $derived.by((): StatusTone => {
    if ($ds.phase === "starting" || $ds.phase === "running") return "live";
    if ($ds.phase === "stopped") return "warn";
    return $ds.exitCode === null || $ds.exitCode === 0 ? "ok" : "bad";
  });
  async function start(): Promise<void> {
    if (!root || !folder || !choice) return;
    const label = choices.find((c) => c.value === choice)?.label ?? choice;
    const target = choice === "file" && fileRel ? ({ kind: "current_file", rel: fileRel } as const) : ({ kind: "named", name: choice.slice(6) } as const);
    open = {};
    const typed = splitArgs(argsByChoice[choice] ?? "");
    await startDebugging(root, folder, target, label, inTerminal, typed.length > 0 ? typed : null);
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
        <span class="caret" aria-hidden="true">
          {#if v.variables_reference > 0}<Icon name={open[v.variables_reference] ? "chevron-down" : "chevron-right"} size="sm" />{/if}
        </span>
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
  <SectionHead label="Debug" rule level={2}>
    {#snippet actions()}
      {#if $ds.phase === "idle" || $ds.phase === "ended"}
        <IconButton icon="refresh-cw" label="Read the configurations again" size="sm" onclick={() => void refresh()} />
      {/if}
    {/snippet}
  </SectionHead>
  <div class="body">
    {#if !root}
      <p class="note">Open a project to debug it.</p>
    {:else}
      {#if listed?.project_file && !listed.project_file.trusted && $ds.phase !== "running" && $ds.phase !== "stopped"}
        {#snippet review()}
          <ul class="review">
            {#each projectConfigs as c (c.name)}<li><code>{c.name}: {describe(c)}</code></li>{/each}
          </ul>
          <div class="actions">
            <button type="button" class="ax-btn primary" onclick={() => void allow()}>Allow these</button>
            <button type="button" class="ax-btn" onclick={() => (reviewing = false)}>Cancel</button>
          </div>
        {/snippet}
        <div class="notice-slot">
          <Notice title="Project debug setups need your review" alert children={reviewing ? review : undefined}>
            {#snippet details()}
              <p>
                This project has its own <code>.axiomata/debug.json</code>. It starts programs from the repository, so it
                stays off until you have looked at them.
              </p>
            {/snippet}
            {#snippet action()}
              {#if !reviewing}
                <button type="button" class="ax-btn" onclick={() => (reviewing = true)}>Review…</button>
              {/if}
            {/snippet}
          </Notice>
        </div>
      {/if}

      {#if !busy}
        <section class="plate g-start">
          <h3>
            <span class="swatch" aria-hidden="true"></span>Start
            {#if chosen && !chosen.detected && !locked(chosen)}
              <span class="head-actions">
                <IconButton icon="pencil" label="Edit {chosen.name}" size="sm" onclick={() => startEdit(chosen)} />
                <IconButton icon="trash-2" label="Remove {chosen.name}" size="sm" onclick={() => void remove(chosen)} />
              </span>
            {/if}
          </h3>
          <div class="launch">
            <div class="start">
              <span class="picker">
                <select bind:value={choice} onchange={() => (picked = true)} aria-label="What to debug" disabled={choices.length === 0}>
                  {#each choices as c (c.value)}<option value={c.value} disabled={c.disabled}>{c.label}</option>{/each}
                </select>
                <span class="picker-caret" aria-hidden="true"><Icon name="chevron-down" size="sm" /></span>
              </span>
              <button type="button" class="ax-btn primary go" disabled={!choice || !folder} onclick={() => void start()}>
                <Icon name="bug" size="sm" /> Debug
              </button>
            </div>
            {#if chosen}<p class="what">{describe(chosen) || chosen.name}</p>{/if}
            <label class="field">
              Arguments
              <input
                class="mono"
                type="text"
                value={argsByChoice[choice] ?? ""}
                oninput={(e) => (argsByChoice[choice] = e.currentTarget.value)}
                placeholder={argsPlaceholder}
                spellcheck="false"
                autocomplete="off"
              />
            </label>
            <label class="toggle" class:on={inTerminal}>
              <input type="checkbox" bind:checked={inTerminal} />
              <span class="toggle-text">
                Run in a terminal
                <small>For a TUI or a program that reads the keyboard</small>
              </span>
              <span class="switch" aria-hidden="true"></span>
            </label>
          </div>
        </section>
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
            <label class="field">
              Name
              <input type="text" bind:value={form.name} placeholder="e.g. OChaT app" spellcheck="false" />
            </label>
            <label class="field">
              File or module
              <input class="mono" type="text" bind:value={form.target} placeholder="src/ocht/main.py or ocht.cli" spellcheck="false" />
            </label>
            <label class="field">
              Arguments <small>optional</small>
              <input class="mono" type="text" bind:value={form.args} spellcheck="false" />
            </label>
            <label class="field">
              Working folder <small>inside the project, optional</small>
              <input class="mono" type="text" bind:value={form.cwd} spellcheck="false" />
            </label>
            <p class="hint">Kept in <code>.axiomata/debug.json</code> of this project.</p>
            <div class="actions">
              <button type="submit" class="ax-btn primary" disabled={saving || !form.name.trim() || !form.target.trim()}>Save</button>
              <button type="button" class="ax-btn" onclick={() => (editing = null)}>Cancel</button>
            </div>
          </form>
        {/if}
        {#each listed?.problems ?? [] as problem (problem)}<p class="problem">{problem}</p>{/each}
      {/if}

      {#if $ds.phase !== "idle"}
        <section class="plate session">
          <div class="session-head">
            <StatusMark tone={sessionTone} />
            <span class="session-text">
              {#if $ds.phase === "starting"}Starting <b>{$ds.configName}</b>…
              {:else if $ds.phase === "running"}Running <b>{$ds.configName}</b>
              {:else if $ds.phase === "stopped"}Paused{$ds.stop ? ` — ${$ds.stop.reason}${$ds.stop.text ? `: ${$ds.stop.text}` : ""}` : ""}
              {:else if $ds.phase === "ended"}Ended{$ds.exitCode !== null ? ` — exit code ${$ds.exitCode}` : ""}
              {/if}
            </span>
          </div>
          {#if busy}
            <div class="toolbar" role="toolbar" aria-label="Debugger">
              {#if $ds.phase === "stopped"}
                <IconButton icon="play" label="Continue ({KEY_LABEL.continue})" size="sm" onclick={() => void debug.control("continue")} />
              {:else}
                <IconButton icon="pause" label="Pause" size="sm" disabled={$ds.phase !== "running"} onclick={() => void debug.control("pause")} />
              {/if}
              <span class="sep" aria-hidden="true"></span>
              <IconButton icon="step-forward" label="Step over ({KEY_LABEL.next})" size="sm" disabled={$ds.phase !== "stopped"} onclick={() => void debug.control("next")} />
              <IconButton icon="arrow-down-to-line" label="Step into ({KEY_LABEL.step_in})" size="sm" disabled={$ds.phase !== "stopped"} onclick={() => void debug.control("step_in")} />
              <IconButton icon="arrow-up-from-line" label="Step out ({KEY_LABEL.step_out})" size="sm" disabled={$ds.phase !== "stopped"} onclick={() => void debug.control("step_out")} />
              <span class="sep" aria-hidden="true"></span>
              <IconButton icon="square" label="Stop" size="sm" onclick={() => void debug.stop()} />
            </div>
          {/if}
        </section>
      {/if}
      {#if $ds.error}<p class="problem">{$ds.error}</p>{/if}
      {#if $debugProblem && $debugProblem !== $ds.error}<p class="problem">{$debugProblem}</p>{/if}

      {#if $ds.phase === "stopped"}
        <section class="plate">
          <h3><span class="swatch" aria-hidden="true"></span>Call stack<span class="n">{$ds.frames.length}</span></h3>
          <ul class="rows">
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
        </section>
        <section class="plate">
          <h3><span class="swatch" aria-hidden="true"></span>Variables</h3>
          <div class="vars">
            {#each $ds.scopes as s (s.scope.variables_reference)}
              {@const ref = s.scope.variables_reference}
              <button type="button" class="scope" onclick={() => void toggleScope(ref, s.scope.expensive)}>
                <span class="caret" aria-hidden="true">
                  <Icon name={(open[ref] ?? !s.scope.expensive) ? "chevron-down" : "chevron-right"} size="sm" />
                </span>{s.scope.name}
              </button>
              {#if (open[ref] ?? !s.scope.expensive) && s.variables}
                <ul>{@render nodes(s.variables, 0)}</ul>
              {/if}
            {/each}
          </div>
        </section>
      {/if}

      <section class="plate g-watch">
        <h3><span class="swatch" aria-hidden="true"></span>Watch<span class="n">{$ds.watches.length}</span></h3>
        {#if $ds.watches.length > 0}
          <ul class="rows">
            {#each $ds.watches as w, i (w.expr)}
              <li class="watch">
                <span class="wexpr">{w.expr}</span>
                <span class="wvalue" class:error={w.error !== null}>
                  {w.error !== null ? w.error : (w.value ?? ($ds.phase === "stopped" ? "…" : ""))}
                </span>
                <span class="row-actions">
                  <IconButton icon="x" label="Remove {w.expr}" size="sm" onclick={() => removeWatch(i)} />
                </span>
              </li>
            {/each}
          </ul>
        {/if}
        <form
          class="inline-form"
          onsubmit={(e) => {
            e.preventDefault();
            addWatch(watchInput);
            watchInput = "";
          }}
        >
          <Icon name="plus" size="sm" />
          <input type="text" bind:value={watchInput} placeholder="Add a watch expression…" spellcheck="false" autocomplete="off" />
        </form>
      </section>

      <section class="plate g-bp">
        <h3>
          <span class="swatch" aria-hidden="true"></span>Breakpoints<span class="n">{total}</span>
          {#if total > 0 && root}
            <IconButton icon="trash-2" label="Remove all breakpoints" size="sm" onclick={() => clearBreakpoints(root)} />
          {/if}
        </h3>
        {#if total === 0}
          <p class="plate-note">Click a line number in a Python or Rust file to set one.</p>
        {:else}
          <ul class="rows">
            {#each files as [rel, lines] (rel)}
              {#each lines as line (line)}
                <li class="bp-row">
                  <div class="bp">
                    <button type="button" class="bp-open" onclick={() => onOpen(rel, line)} title={rel}>
                      <span class="dot" class:plain={!infoOf($breakpointInfo, root ?? "", rel, line)} aria-hidden="true"></span>
                      <span class="bp-file">{leaf(rel)}</span><span class="bp-line">{line}</span>
                    </button>
                    <span class="row-actions">
                      <IconButton
                        icon="pencil"
                        label="Condition, hit count, log message"
                        size="sm"
                        onclick={() => (editingBp = editingBp?.rel === rel && editingBp.line === line ? null : { rel, line })}
                      />
                      <IconButton icon="x" label="Remove" size="sm" onclick={() => root && toggleDebugBreakpoint(root, rel, line)} />
                    </span>
                  </div>
                  {#if infoOf($breakpointInfo, root ?? "", rel, line)}
                    {@const extra = infoOf($breakpointInfo, root ?? "", rel, line)}
                    <p class="bp-extra">
                      {#if extra?.condition}when <code>{extra.condition}</code>{/if}
                      {#if extra?.hit}hit <code>{extra.hit}</code>{/if}
                      {#if extra?.log}logs <code>{extra.log}</code>{/if}
                    </p>
                  {/if}
                  {#if editingBp?.rel === rel && editingBp.line === line && root}
                    <div class="bp-form">
                      <BreakpointEditor
                        info={infoOf($breakpointInfo, root, rel, line)}
                        label="{leaf(rel)}:{line}"
                        onSave={(info) => editDebugBreakpoint(root, rel, line, info)}
                        onClose={() => (editingBp = null)}
                      />
                    </div>
                  {/if}
                </li>
              {/each}
            {/each}
          </ul>
        {/if}
      </section>

      {#if $ds.tui}
        <p class="problem">
          This program draws a full-screen interface, which a console cannot show. Stop it and start again with
          “Run in a terminal” ticked — breakpoints and stepping work the same there.
        </p>
      {/if}
      {#if busy || $ds.output.length > 0}
        <section class="plate">
          <h3><span class="swatch" aria-hidden="true"></span>Console</h3>
          <div class="console" bind:this={consoleEl}>
            {#each $ds.output as line, i (i)}<span class="out {line.category}">{line.text}</span>{/each}
          </div>
          {#if $ds.phase === "stopped"}
            <form
              class="inline-form"
              onsubmit={(e) => {
                e.preventDefault();
                void debug.evaluate(expression);
                expression = "";
              }}
            >
              <Icon name="chevron-right" size="sm" />
              <input type="text" bind:value={expression} placeholder="Evaluate in this frame…" spellcheck="false" autocomplete="off" />
            </form>
          {/if}
        </section>
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

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: var(--ax-space-3);
  }

  /* Every part of the view is a plate with a head, as in Run and Git; the tone marks the kind of thing on it. */
  .plate {
    --tone: var(--ax-text-muted);
    margin-bottom: var(--ax-space-3);
    overflow: hidden;
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-elev-2);
    box-shadow: var(--ax-shadow-raised);
  }

  .g-start {
    --tone: var(--ax-text);
  }

  .g-watch {
    --tone: var(--ax-harness-opencode);
  }

  .g-bp {
    --tone: var(--ax-danger);
  }

  h3 {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    min-height: calc(30px * var(--ax-ui-scale));
    margin: 0;
    padding: var(--ax-space-1) var(--ax-space-2) var(--ax-space-1) var(--ax-space-3);
    border-bottom: 1px solid var(--ax-border);
    background: color-mix(in srgb, var(--tone) 5%, var(--ax-elev-3));
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    font-weight: 600;
    box-sizing: border-box;
  }

  .swatch {
    flex: none;
    width: calc(8px * var(--ax-ui-scale));
    height: calc(8px * var(--ax-ui-scale));
    clip-path: var(--ax-hex-clip);
    background: var(--tone);
  }

  .n {
    margin-left: auto;
    padding: 0 var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-3);
    font-weight: 400;
    letter-spacing: 0;
  }

  .head-actions {
    display: flex;
    gap: var(--ax-space-1);
    margin-left: auto;
    letter-spacing: 0;
    text-transform: none;
  }

  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .rows > li + li {
    border-top: 1px solid color-mix(in srgb, var(--ax-border) 60%, transparent);
  }

  /* ---- start ---- */

  .launch {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-3);
    padding: var(--ax-space-3);
  }

  .start {
    display: flex;
    gap: var(--ax-space-2);
  }

  .picker {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
  }

  .picker select {
    flex: 1;
    min-width: 0;
    padding: var(--ax-space-2) calc(var(--ax-space-2) * 2 + 14px * var(--ax-ui-scale)) var(--ax-space-2) var(--ax-space-3);
    appearance: none;
    background: var(--ax-bg);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    color: var(--ax-text);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    font-weight: 600;
    cursor: pointer;
  }

  .picker select:hover:not(:disabled) {
    background: var(--ax-elev-3);
  }

  .picker-caret {
    position: absolute;
    top: 50%;
    right: var(--ax-space-2);
    display: flex;
    transform: translateY(-50%);
    color: var(--ax-text-muted);
    pointer-events: none;
  }

  .go {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    white-space: nowrap;
  }

  .what {
    margin: calc(-1 * var(--ax-space-2)) 0 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .field small {
    opacity: 0.8;
  }

  input[type="text"] {
    min-width: 0;
    padding: var(--ax-space-2);
    background: var(--ax-bg);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
  }

  input.mono {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  select:focus-visible,
  input[type="text"]:focus-visible {
    outline: var(--ax-focus-ring);
  }

  /* "Run in a terminal" is a tile with a switch: a choice about how the program runs, not a footnote. */
  .toggle {
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
    padding: var(--ax-space-2) var(--ax-space-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    color: var(--ax-text);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .toggle:hover {
    background: var(--ax-elev-3);
  }

  .toggle input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .toggle:focus-within {
    outline: var(--ax-focus-ring);
  }

  .toggle.on {
    border-color: var(--ax-border-strong);
    background: var(--ax-elev-3);
    color: var(--ax-text);
  }

  .toggle-text {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: calc(2px * var(--ax-ui-scale));
  }

  .toggle-text small {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .switch {
    position: relative;
    flex: none;
    width: calc(28px * var(--ax-ui-scale));
    height: calc(16px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-3);
    box-shadow: inset 0 0 0 1px var(--ax-border-strong);
    transition: background var(--ax-dur-fast) var(--ax-ease);
  }

  .switch::after {
    content: "";
    position: absolute;
    top: calc(2px * var(--ax-ui-scale));
    left: calc(2px * var(--ax-ui-scale));
    width: calc(12px * var(--ax-ui-scale));
    height: calc(12px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-pill);
    background: var(--ax-text-muted);
    transition:
      transform var(--ax-dur-fast) var(--ax-ease),
      background var(--ax-dur-fast) var(--ax-ease);
  }

  /* On is a setting, not something alive: the text colour, never the accent. */
  .toggle.on .switch {
    background: color-mix(in srgb, var(--ax-text) 25%, transparent);
  }

  .toggle.on .switch::after {
    background: var(--ax-text);
    transform: translateX(calc(12px * var(--ax-ui-scale)));
  }

  .add {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--ax-space-2);
    width: 100%;
    margin-bottom: var(--ax-space-3);
    padding: var(--ax-space-2);
    background: none;
    border: 1px dashed var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .add:hover,
  .add:focus-visible {
    border-style: solid;
    background: var(--ax-elev-2);
    color: var(--ax-text);
  }

  form.config {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-3);
    margin-bottom: var(--ax-space-3);
    padding: var(--ax-space-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-elev-2);
    box-shadow: var(--ax-shadow-raised);
  }

  .form-title {
    margin: 0;
    color: var(--ax-text);
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
  }

  .hint {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  /* ---- the session ---- */

  .session-head {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2) var(--ax-space-3);
    font-size: var(--ax-font-size-sm);
  }

  .session-text {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
    color: var(--ax-text);
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1) var(--ax-space-2);
    border-top: 1px solid var(--ax-border);
    background: var(--ax-elev-3);
  }

  .sep {
    width: 1px;
    height: calc(16px * var(--ax-ui-scale));
    margin: 0 var(--ax-space-1);
    background: var(--ax-border);
  }

  .frame,
  .var,
  .scope,
  .bp-open {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-2);
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-3);
    background: none;
    border: none;
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
    background: var(--ax-elev-3);
  }

  /* Code outside the project (the standard library, generated files): shown, but not one to open. */
  .frame.outside .fname {
    color: var(--ax-text-muted);
    font-style: italic;
  }

  .frame.on {
    background: var(--ax-selected-bg);
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

  .vars {
    padding: var(--ax-space-1) 0;
  }

  .scope {
    align-items: center;
    gap: var(--ax-space-1);
    padding-left: var(--ax-space-2);
    font-weight: 600;
  }

  .caret {
    flex: 0 0 auto;
    display: inline-flex;
    width: 1em;
    color: var(--ax-text-muted);
  }

  .var {
    align-items: center;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .vname {
    flex: 0 0 auto;
    color: var(--ax-harness-opencode);
  }

  .vvalue {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ax-text);
  }

  /* ---- watch ---- */

  .watch {
    position: relative;
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-3);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .wexpr {
    flex: 0 0 auto;
    max-width: 45%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--tone);
  }

  .wvalue {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .wvalue.error {
    color: var(--ax-warning);
  }

  /* A row's own buttons appear on hover, over its right end. */
  .row-actions {
    position: absolute;
    top: 50%;
    right: var(--ax-space-2);
    display: flex;
    gap: var(--ax-space-1);
    transform: translateY(-50%);
    opacity: 0;
    background: var(--ax-surface-3);
    border-radius: var(--ax-radius-md);
  }

  li:hover .row-actions,
  li:focus-within .row-actions {
    opacity: 1;
  }

  /* An input at the foot of a plate: a row of the plate, not a box of its own. */
  .inline-form {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: 0 var(--ax-space-3);
    border-top: 1px solid color-mix(in srgb, var(--ax-border) 60%, transparent);
    color: var(--ax-text-muted);
  }

  .plate > h3 + .inline-form {
    border-top: none;
  }

  .inline-form input[type="text"] {
    flex: 1;
    padding: var(--ax-space-2) 0;
    background: none;
    border: none;
    border-radius: 0;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .inline-form input[type="text"]:focus-visible {
    outline: none;
  }

  .inline-form:focus-within {
    background: var(--ax-elev-3);
    color: var(--ax-text);
  }

  /* ---- breakpoints ---- */

  .plate-note {
    margin: 0;
    padding: var(--ax-space-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  .bp-row {
    display: flex;
    flex-direction: column;
  }

  .bp {
    position: relative;
    display: flex;
    align-items: center;
  }

  .bp-open {
    flex: 1;
    min-width: 0;
    align-items: center;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .bp-file {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bp-line {
    flex: none;
    padding: 0 var(--ax-space-2);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-3);
    color: var(--ax-text-muted);
  }

  .dot {
    flex: 0 0 auto;
    width: calc(9px * var(--ax-ui-scale));
    height: calc(9px * var(--ax-ui-scale));
    clip-path: var(--ax-hex-clip);
    background: var(--ax-danger);
  }

  /* A breakpoint with a condition, hit count or log message: the hexagon's ring only. */
  .dot:not(.plain) {
    background: transparent;
    box-shadow: inset 0 0 0 calc(2px * var(--ax-ui-scale)) var(--ax-danger);
    clip-path: none;
    border-radius: var(--ax-radius-pill);
  }

  .bp-extra {
    margin: 0 var(--ax-space-3) var(--ax-space-2) calc(9px * var(--ax-ui-scale) + var(--ax-space-3) + var(--ax-space-2));
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .bp-extra code {
    font-family: var(--ax-font-mono);
    color: var(--ax-text);
  }

  .bp-form {
    padding: var(--ax-space-2);
    margin: 0 var(--ax-space-2) var(--ax-space-2);
    background: var(--ax-elev-3);
    border-radius: var(--ax-radius-md);
  }

  /* ---- console ---- */

  .console {
    max-height: calc(220px * var(--ax-ui-scale));
    overflow-y: auto;
    padding: var(--ax-space-2) var(--ax-space-3);
    background: var(--ax-bg);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .console:empty {
    display: none;
  }

  .out.stderr {
    color: var(--ax-danger);
  }

  .out.repl {
    color: var(--ax-harness-opencode);
  }

  .out.console {
    color: var(--ax-text-muted);
  }

  /* ---- loose text ---- */

  .note,
  .problem {
    margin: 0 0 var(--ax-space-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  .problem {
    color: var(--ax-warning);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .notice-slot {
    margin-bottom: var(--ax-space-3);
  }

  .actions {
    display: flex;
    gap: var(--ax-space-2);
  }

  code {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }
</style>

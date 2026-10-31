<!--
  The permanent bottom input line. Enter routes the text (core/commands, #108): an app command runs locally, an
  engine command and plain text go to the chat, `/do <text>` is a one-shot agent instruction, an unknown `/x` is
  refused with the nearest names. While the first word after `/` is typed, a list of matching commands opens above
  the bar: ↑/↓ move in it, Tab takes one, Enter takes it unless the line already names a command, Esc closes it.
  Once a list command has its space (`/model `, `/skills `, `/resume `, `/help `, `/effort `), its list opens in the
  panel above the bar (`SlashPanel.svelte`, #109) and what is typed after it filters the list; ↑/↓ choose, Enter takes
  the row (a skill asks first), Esc or a click beside it closes it. A command's answer (`/cost`, `/undo`, a tile
  action's output) opens there too — nothing of a command is written into the conversation.
  Otherwise ↑/↓ walk the input history.

  The conversation's settings sit at the bar's right end, so they can be set before the first question: Plan/Build,
  Glut's effort (`EffortSwitch.svelte`) and a new conversation; in the input ⇧⇥ switches Plan/Build and ⌘N starts
  afresh (`core/chatKeys.ts`).
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";

  import { CONFIG_SAVED } from "../core/backend";
  import { on } from "../core/bus";
  import {
    busy,
    interruptedTurns,
    loadInterruptedTurns,
    newSession,
    panelOpen,
    planMode,
    send,
    turns,
  } from "../core/chat";
  import { isModeTogglePress, isNewSessionPress } from "../core/chatKeys";
  import {
    type CommandEntry,
    commandEntries,
    engineCommands,
    refreshChatHarness,
    refreshEngineCommands,
    route,
    runCommand,
    suggestions,
  } from "../core/commands";
  import { keyHints } from "../core/keyHints";
  import { listQuery, shownRows, type Panel, type PanelRow } from "../core/slashLists";
  import { tChat } from "../i18n/chat";
  import { platform } from "../core/keys";
  import { toast } from "../core/toast";
  import IconButton from "../ui/IconButton.svelte";
  import StatusMark from "../ui/StatusMark.svelte";
  import EffortSwitch from "./EffortSwitch.svelte";
  import SlashPanel from "./SlashPanel.svelte";

  /** How old the engine's list may get before typing `/` reads it again. */
  const REFRESH_AFTER_MS = 30_000;

  let input = $state("");
  let history = $state<string[]>([]);
  let cursor = $state(-1);
  let picked = $state(0);
  let dismissed = $state(false);
  let refreshedAt = 0;

  const placeholder = $derived($busy ? tChat("placeholderBusy") : tChat("placeholder"));
  const shown = $derived<CommandEntry[]>(dismissed ? [] : suggestions(input, $engineCommands));
  const listOpen = $derived(shown.length > 0);

  /** The panel above the bar (#109): a list command's list, or a command's answer. */
  let panel = $state<Panel | null>(null);
  /** The panel follows the line (`/model …`): typing filters it, leaving the command closes it. */
  let live = $state(false);
  let panelPicked = $state(0);
  /** The row whose "Run …?" is open; a second Enter on it runs it. */
  let confirming = $state<string | null>(null);
  /** Counts list reads, so a slow answer for a command already left behind is dropped. */
  let listRead = 0;
  /** The list command last asked for, so a command that gave no list is not asked again at every key. */
  let askedFor: string | null = null;
  let dock = $state<HTMLElement | undefined>();

  const query = $derived(listQuery(input));
  const filter = $derived(live && query && panel?.command === query.command ? query.filter : "");
  const rows = $derived<PanelRow[]>(panel ? shownRows(panel, filter) : []);

  function closePanel(): void {
    panel = null;
    live = false;
    confirming = null;
    askedFor = null;
    listRead++;
  }

  /** Opens the list of `command` while its argument is typed; read once per command, filtered after that. */
  async function openList(command: string): Promise<void> {
    const mine = ++listRead;
    askedFor = command;
    const result = await runCommand(command, []);
    if (mine !== listRead) return;
    // An old list never stands for a new command: without a list of its own, the panel closes.
    panel = null;
    live = false;
    if (result.panel) {
      panel = result.panel;
      live = true;
      panelPicked = 0;
      confirming = null;
    } else if (result.message) {
      toast(result.message, result.ok ? "info" : "warning");
    }
  }

  // A chat turn the app lost is said once at start; the chat panel offers to finish it (durable runs).
  // A harness changed in Settings changes which commands are offered (`/effort` is Glut's).
  onMount(() => on(CONFIG_SAVED, () => void refreshChatHarness()));

  onMount(() => {
    void loadInterruptedTurns().then(() => {
      if (get(interruptedTurns).length === 0) return;
      toast(tChat("turnStopped"), "warning");
    });
  });

  // Read when first needed, not at start: the list is only wanted once the owner types `/`.
  function refresh(): void {
    refreshedAt = Date.now();
    void refreshEngineCommands();
    void refreshChatHarness();
  }

  function oninput(): void {
    dismissed = false;
    picked = 0;
    panelPicked = 0;
    confirming = null;
    if (input.startsWith("/") && Date.now() - refreshedAt > REFRESH_AFTER_MS) refresh();
    syncPanel();
  }

  /** The panel follows the line: a list command opens its list once, any other line closes it (and drops a read). */
  function syncPanel(): void {
    const q = listQuery(input);
    if (q && askedFor !== q.command) void openList(q.command);
    else if (!q) closePanel();
  }

  /** A row taken: it runs its command line, asks first, or puts its command into the bar to be finished. */
  function pickRow(row: PanelRow): void {
    if (!row.line) return;
    if (row.confirm && confirming !== row.id) {
      confirming = row.id;
      return;
    }
    if (!row.run) {
      input = row.line;
      closePanel();
      oninput();
      return;
    }
    closePanel();
    input = "";
    void execute(row.line);
  }

  // A click beside the bar and its panel closes the panel.
  onMount(() => {
    const away = (e: PointerEvent) => {
      if (panel && dock && e.target instanceof Node && !dock.contains(e.target)) closePanel();
    };
    window.addEventListener("pointerdown", away, true);
    return () => window.removeEventListener("pointerdown", away, true);
  });

  function take(entry: CommandEntry): void {
    input = `/${entry.name} `;
    dismissed = true;
    syncPanel();
  }

  /** Whether the line's first word already names a command, so Enter sends it rather than taking a suggestion. */
  function namesCommand(line: string): boolean {
    const typed = line.slice(1).toLowerCase();
    return commandEntries($engineCommands).some((c) => c.name.toLowerCase() === typed);
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (listOpen && !namesCommand(input)) {
      const entry = shown[picked];
      if (entry) take(entry);
      return;
    }
    // A list in the panel takes Enter for its picked row; with nothing matching, the line goes as typed (Q18).
    const row = panel?.kind === "list" ? rows[panelPicked] : undefined;
    if (row?.line) {
      if (input.trim()) history = [input, ...history.filter((h) => h !== input)].slice(0, 50);
      pickRow(row);
      return;
    }
    const line = input;
    if (!route(line)) return;
    history = [line, ...history.filter((h) => h !== line)].slice(0, 50);
    cursor = -1;
    input = "";
    dismissed = false;
    closePanel();
    await execute(line);
  }

  /** Runs a bar line: an app command here (its answer in the panel), everything else as a chat turn. */
  async function execute(line: string): Promise<void> {
    const routed = route(line);
    if (!routed) return;
    switch (routed.kind) {
      case "command": {
        const result = await runCommand(routed.name, routed.args);
        if (result.message) toast(result.message, result.ok ? "info" : "warning");
        if (result.panel) {
          panel = result.panel;
          live = false;
          panelPicked = 0;
          confirming = null;
        }
        return;
      }
      case "unknown":
        toast(routed.message, "warning");
        return;
      case "engine":
        await send(routed.args, "chat", { kind: "run", name: routed.name });
        break;
      default:
        await send(routed.message, routed.kind);
    }
    // Claude Code lists its commands with each turn, and a turn may add a command file: read the list again.
    refresh();
  }

  /** Nothing to start afresh while the agent works or before the first turn. */
  const canStartOver = $derived(!$busy && $turns.length > 0);

  function onKeydown(e: KeyboardEvent) {
    if (isNewSessionPress(e, $platform)) {
      e.preventDefault();
      if (canStartOver) newSession();
      return;
    }
    if (isModeTogglePress(e)) {
      e.preventDefault();
      planMode.update((planning) => !planning);
      return;
    }
    if (panel && !listOpen) {
      if ((e.key === "ArrowDown" || e.key === "ArrowUp") && rows.length > 0) {
        const step = e.key === "ArrowDown" ? 1 : -1;
        panelPicked = (panelPicked + step + rows.length) % rows.length;
        confirming = null;
        e.preventDefault();
        return;
      }
      if (e.key === "Escape") {
        closePanel();
        e.preventDefault();
        return;
      }
    }
    if (listOpen) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        const step = e.key === "ArrowDown" ? 1 : -1;
        picked = (picked + step + shown.length) % shown.length;
        e.preventDefault();
        return;
      }
      if (e.key === "Tab") {
        const entry = shown[picked];
        if (entry) take(entry);
        e.preventDefault();
        return;
      }
      if (e.key === "Escape") {
        dismissed = true;
        e.preventDefault();
        return;
      }
    }
    // A recalled `/help` must not open the list: it would take the next arrow key from the history.
    if (e.key === "ArrowUp" && history.length > 0) {
      cursor = Math.min(cursor + 1, history.length - 1);
      input = history[cursor] ?? "";
      dismissed = true;
      closePanel();
      e.preventDefault();
    } else if (e.key === "ArrowDown") {
      cursor = Math.max(cursor - 1, -1);
      input = cursor === -1 ? "" : (history[cursor] ?? "");
      dismissed = true;
      closePanel();
      e.preventDefault();
    }
  }

  // The top-bar search icon opens the spotlight (`shell:spotlight`, `App.svelte`), not this chat field.
</script>

<div class="dock" bind:this={dock}>
<form class="bar" class:thinking={$busy} onsubmit={submit}>
  {#if panel && !listOpen}
    <SlashPanel {panel} {rows} {filter} picked={panelPicked} {confirming} onPick={pickRow} onClose={closePanel} />
  {/if}
  {#if listOpen}
    <ul id="assistant-commands" class="suggestions" role="listbox" aria-label={tChat("commandsLabel")}>
      {#each shown as entry, index (`${entry.source}:${entry.name}`)}
        <li
          role="option"
          aria-selected={index === picked}
          class:picked={index === picked}
          onmousedown={(e) => {
            e.preventDefault();
            take(entry);
          }}
        >
          <span class="name">/{entry.name}</span>
          {#if entry.hint}<span class="hint">{entry.hint}</span>{/if}
          <span class="description">{entry.description}</span>
          <span class="source">{entry.source}</span>
        </li>
      {/each}
    </ul>
  {/if}
  <span class="prompt" aria-hidden="true">›</span>
  <input
    bind:value={input}
    type="text"
    {placeholder}
    aria-label={tChat("inputLabel")}
    role="combobox"
    aria-autocomplete="list"
    aria-controls={panel && !listOpen ? "assistant-panel" : "assistant-commands"}
    aria-expanded={listOpen || (panel?.kind === "list" && rows.length > 0)}
    aria-activedescendant={panel?.kind === "list" && !listOpen && rows[panelPicked]
      ? `assistant-panel-row-${panelPicked}`
      : undefined}
    autocomplete="off"
    spellcheck="false"
    oninput={oninput}
    onkeydown={onKeydown}
  />
  <div class="controls">
    <button
      type="button"
      class="ax-btn mode"
      class:on={$planMode}
      aria-pressed={$planMode}
      title={$planMode ? tChat("planTitle") : tChat("buildTitle")}
      onclick={() => planMode.update((planning) => !planning)}
    >
      {$planMode ? tChat("plan") : tChat("build")}
    </button>
    <EffortSwitch />
    <IconButton
      icon="square-pen"
      size="sm"
      label={tChat("newSession", { keys: keyHints("⌘N", $platform) })}
      disabled={!canStartOver}
      onclick={newSession}
    />
  </div>
  {#if $busy}
    <span class="chip"><StatusMark tone="live" />{tChat("working")}</span>
  {:else if $turns.length > 0}
    <button
      type="button"
      class="toggle"
      title={tChat("messages", { n: $turns.length })}
      onclick={() => panelOpen.update((v) => !v)}
    >
      <span class="toggle-label">{$panelOpen ? tChat("hideChat") : tChat("showChat")}</span>
      <span class="toggle-count" aria-hidden="true">{$turns.length}</span>
    </button>
  {/if}
</form>
</div>

<style>
  /* Centred pill — never the full width of an ultra-wide monitor. */
  .dock {
    flex: 0 0 auto;
    display: flex;
    justify-content: center;
    padding: var(--ax-space-2) var(--ax-space-4) var(--ax-space-3);
  }
  /* While the agent works an ember runs round the bar ("Glut + Hex", UX3): a conic gradient in the border box under
     the bar's own fill, turned by `--ax-anim-orbit` (still at Calm and Off — the ember then simply stands). */
  .bar {
    position: relative;
    width: min(var(--ax-assistant-width), 100%);
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-3) var(--ax-space-1) var(--ax-space-4);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-1);
    box-shadow: var(--ax-shadow-tile);
    z-index: var(--ax-z-assistant);
  }
  .bar:focus-within {
    border-color: var(--ax-text-muted);
  }
  .bar.thinking {
    border-color: transparent;
    background:
      linear-gradient(var(--ax-surface-1), var(--ax-surface-1)) padding-box,
      conic-gradient(
          from var(--ax-orbit-angle),
          transparent 0deg,
          var(--ax-glow) 60deg,
          transparent 140deg,
          transparent 360deg
        )
        border-box;
    box-shadow:
      var(--ax-shadow-tile),
      var(--ax-glow-shadow);
    animation: var(--ax-anim-orbit);
  }
  .prompt {
    color: var(--ax-text-muted);
    font-weight: 700;
  }
  .thinking .prompt {
    color: var(--ax-glow);
  }
  input {
    flex: 1 1 auto;
    min-width: 0;
    background: transparent;
    border-color: transparent;
    padding: var(--ax-space-1) var(--ax-space-2);
  }
  input:focus-visible {
    outline: none;
    border-color: transparent;
  }
  .controls {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-2);
  }
  .mode {
    font-size: var(--ax-font-size-sm);
    padding: calc(1px * var(--ax-ui-scale)) var(--ax-space-2);
  }
  /* Selected, not alive: the switch is a setting, so it takes the selection tone, never the glow. */
  .mode.on {
    background: var(--ax-selected-bg);
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    padding: 1px var(--ax-space-3);
    border-radius: var(--ax-radius-pill);
  }
  .toggle-label {
    color: var(--ax-text);
  }
  /* Turn count as a distinct badge, not a "chat #N" label — the panel has a
     single session, so a bare number reads like the conversation's size. */
  .toggle-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.2em;
    height: 1.2em;
    padding: 0 0.3em;
    border-radius: var(--ax-radius-pill);
    background: var(--ax-selected-bg);
    color: var(--ax-text);
    font-size: 0.85em;
    font-weight: 600;
  }
  /* The command list stands on the bar, as wide as it, and scrolls when long. */
  .suggestions {
    position: absolute;
    left: 0;
    right: 0;
    bottom: calc(100% + var(--ax-space-2));
    max-height: calc(320px * var(--ax-ui-scale));
    overflow-y: auto;
    margin: 0;
    padding: var(--ax-space-1);
    list-style: none;
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-1);
    box-shadow: var(--ax-shadow-pop);
    z-index: var(--ax-z-popover);
  }
  .suggestions li {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-2);
    padding: var(--ax-space-1) var(--ax-space-2);
    border-radius: var(--ax-radius-sm);
    cursor: pointer;
    font-size: var(--ax-font-size-sm);
  }
  .suggestions li.picked {
    background: var(--ax-selected-bg);
  }
  .name {
    font-family: var(--ax-font-mono);
    color: var(--ax-text);
    white-space: nowrap;
  }
  .hint {
    font-family: var(--ax-font-mono);
    color: var(--ax-text-muted);
    white-space: nowrap;
  }
  .description {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ax-text-muted);
  }
  .source {
    color: var(--ax-text-muted);
    font-size: 0.85em;
  }
</style>

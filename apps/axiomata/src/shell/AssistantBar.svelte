<!--
  The permanent bottom input line. Enter routes the text (core/commands, #108): an app command runs locally, an
  engine command and plain text go to the chat, `/do <text>` is a one-shot agent instruction, an unknown `/x` is
  refused with the nearest names. While the first word after `/` is typed, a list of matching commands opens above
  the bar: ↑/↓ move in it, Tab takes one, Enter takes it unless the line already names a command, Esc closes it.
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
    pushInstruction,
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
  import { platform } from "../core/keys";
  import { toast } from "../core/toast";
  import IconButton from "../ui/IconButton.svelte";
  import StatusMark from "../ui/StatusMark.svelte";
  import EffortSwitch from "./EffortSwitch.svelte";

  /** How old the engine's list may get before typing `/` reads it again. */
  const REFRESH_AFTER_MS = 30_000;

  let input = $state("");
  let history = $state<string[]>([]);
  let cursor = $state(-1);
  let picked = $state(0);
  let dismissed = $state(false);
  let refreshedAt = 0;

  const placeholder = $derived(
    $busy ? "Agent is working…" : "Ask the agent, or /help for commands",
  );
  const shown = $derived<CommandEntry[]>(dismissed ? [] : suggestions(input, $engineCommands));
  const listOpen = $derived(shown.length > 0);

  // A chat turn the app lost is said once at start; the chat panel offers to finish it (durable runs).
  // A harness changed in Settings changes which commands are offered (`/effort` is Glut's).
  onMount(() => on(CONFIG_SAVED, () => void refreshChatHarness()));

  onMount(() => {
    void loadInterruptedTurns().then(() => {
      if (get(interruptedTurns).length === 0) return;
      toast("A chat turn stopped when the app did — open the chat to resume it.", "warning");
    });
  });

  // Read when first needed, not at start: on Opencode it connects to the service, which would start it with the app.
  function refresh(): void {
    refreshedAt = Date.now();
    void refreshEngineCommands();
    void refreshChatHarness();
  }

  function oninput(): void {
    dismissed = false;
    picked = 0;
    if (input.startsWith("/") && Date.now() - refreshedAt > REFRESH_AFTER_MS) refresh();
  }

  function take(entry: CommandEntry): void {
    input = `/${entry.name} `;
    dismissed = true;
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
    const routed = route(input);
    if (!routed) return;
    history = [input, ...history.filter((h) => h !== input)].slice(0, 50);
    cursor = -1;
    input = "";
    dismissed = false;

    switch (routed.kind) {
      case "command": {
        const result = await runCommand(routed.name, routed.args);
        toast(result.message, result.ok ? "info" : "warning");
        if (result.detail) pushInstruction(result.detail);
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
      e.preventDefault();
    } else if (e.key === "ArrowDown") {
      cursor = Math.max(cursor - 1, -1);
      input = cursor === -1 ? "" : (history[cursor] ?? "");
      dismissed = true;
      e.preventDefault();
    }
  }

  // The top-bar search icon opens the spotlight (`shell:spotlight`, `App.svelte`), not this chat field.
</script>

<div class="dock">
<form class="bar" class:thinking={$busy} onsubmit={submit}>
  {#if listOpen}
    <ul id="assistant-commands" class="suggestions" role="listbox" aria-label="Commands">
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
    aria-label="Assistant input"
    role="combobox"
    aria-autocomplete="list"
    aria-controls="assistant-commands"
    aria-expanded={listOpen}
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
      title={$planMode
        ? "Plan mode: the agent reads and plans, changes nothing — click (or ⇧⇥ in the field) to let it build"
        : "Build mode: the agent may change things — click (or ⇧⇥ in the field) to only plan"}
      onclick={() => planMode.update((planning) => !planning)}
    >
      {$planMode ? "Plan" : "Build"}
    </button>
    <EffortSwitch />
    <IconButton
      icon="square-pen"
      size="sm"
      label="New session ({keyHints('⌘N', $platform)} in this field)"
      disabled={!canStartOver}
      onclick={newSession}
    />
  </div>
  {#if $busy}
    <span class="chip"><StatusMark tone="live" />working…</span>
  {:else if $turns.length > 0}
    <button
      type="button"
      class="toggle"
      title="{$turns.length} messages in this conversation"
      onclick={() => panelOpen.update((v) => !v)}
    >
      <span class="toggle-label">{$panelOpen ? "hide chat" : "chat"}</span>
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
    text-transform: uppercase;
  }
</style>

<!--
  terminal — Checkpoint 2 of docs/plans/terminal.md: output is no longer raw
  bytes. `axiomata-terminal`'s `Terminal` (a `vte` parser + a `Screen` cell
  grid) interprets the shell's escape sequences server-side, and every
  update replaces this tile's `<pre>` content with the *current* screen's
  plain text (one join('\n') of `Screen::to_lines()`), not an append of
  what just arrived — the model, not the byte stream, is the source of
  truth for what's on screen, so a redraw (cursor move + overwrite, erase,
  …) just looks like a redraw instead of leftover text underneath it. Still
  no colour/attributes and no visible cursor — the screen model tracks both
  already, but drawing them is Checkpoint 3's canvas renderer; this interim
  `<pre>` only prints characters.

  Input has no local echo: keystrokes are forwarded to the shell as bytes
  (via the hidden-ish text field below, which is cleared on every native
  `input` event rather than trusted as terminal state) and what's typed only
  appears once the shell's own PTY echoes it back over `on_output` — same as
  any real terminal with local echo off. Only Enter/Backspace/Tab and
  Ctrl+<letter> get their own C0 control byte; arrow-key history navigation
  and other escape-sequence input are a later checkpoint's concern, once
  there's a cursor-aware screen model worth navigating.

  Row/column count is a one-time rough guess from the tile's own pixel size
  at mount (an assumed average monospace cell, not a real font metric) —
  Checkpoint 3 replaces this with an actual measured character cell and
  wires live resize (`terminal_resize`, already on the backend but unused
  here) to the tile's own resize events.

  Two outcomes are handled very differently (architecture review,
  Checkpoint 1): the shell ending — typing `exit`, or a write hitting a
  session the backend already tore down — is completely ordinary and just
  appends an inline "[process exited]" banner to the scrollback, the way a
  real terminal multiplexer would, and stops forwarding further keystrokes;
  only `terminal_spawn` itself failing (no shell could even be started) is
  treated as a fatal error that replaces the tile's content, since there's
  nothing usable left to show underneath it.
-->
<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Channel } from "@tauri-apps/api/core";

  import type { ModuleContext } from "../core/types";

  let { ctx }: { ctx: ModuleContext } = $props();

  // Rough monospace cell size in CSS px, only for the initial row/column
  // guess below — see the component doc comment.
  const CHAR_W = 8;
  const CHAR_H = 17;
  const MIN_ROWS = 4;
  const MIN_COLS = 20;

  /** Mirrors the backend's `TerminalEvent` (`src-tauri/src/terminal.rs`).
   *  Field names are snake_case, not camelCase: this rides over a raw
   *  `Channel` payload, not a `#[tauri::command]` argument list, so none of
   *  `invoke`'s usual camelCase<->snake_case bridging applies here. */
  type TerminalEvent =
    | { type: "screen"; lines: string[]; cursor_row: number; cursor_col: number }
    | { type: "exited" };

  let root = $state<HTMLDivElement>();
  let outputEl = $state<HTMLPreElement>();
  let inputEl = $state<HTMLInputElement>();
  let output = $state("");
  let error = $state("");
  let sessionId: string | null = null;

  const encoder = new TextEncoder();

  function cellsFor(width: number, height: number): { rows: number; cols: number } {
    return {
      rows: Math.max(MIN_ROWS, Math.floor(height / CHAR_H)),
      cols: Math.max(MIN_COLS, Math.floor(width / CHAR_W)),
    };
  }

  function scrollToEnd(): void {
    queueMicrotask(() => {
      if (outputEl) outputEl.scrollTop = outputEl.scrollHeight;
    });
  }

  /** The shell is gone (told us so via a `TerminalEvent.exited`, or a write
   *  against it just failed, which means the same thing) — stop treating
   *  `sessionId` as live and say so, calmly, like a real terminal. There's
   *  no further screen snapshot coming once this fires, so appending to
   *  whatever `output` currently holds (the last real snapshot) rather than
   *  replacing it is safe and permanent, not just until the next update. */
  function markEnded(): void {
    if (!sessionId) return;
    sessionId = null;
    output += "\n[process exited]\n";
    scrollToEnd();
  }

  function sendBytes(bytes: Uint8Array): void {
    if (!sessionId) return;
    void ctx.invoke("terminal_write", { id: sessionId, data: Array.from(bytes) }).catch(markEnded);
  }

  async function spawn(): Promise<void> {
    const box = root?.getBoundingClientRect();
    const { rows, cols } = cellsFor(box?.width ?? 640, box?.height ?? 400);

    const onOutput = new Channel<TerminalEvent>();
    onOutput.onmessage = (event) => {
      if (event.type === "exited") {
        markEnded();
        return;
      }
      // Replace, not append: `event.lines` is the *whole* current screen,
      // not a new chunk — see the component doc comment.
      output = event.lines.join("\n");
      scrollToEnd();
    };

    try {
      sessionId = await ctx.invoke<string>("terminal_spawn", { rows, cols, onOutput });
    } catch (err) {
      error = String(err);
    }
  }

  /** Keys that either produce no native `input` event or need a specific C0
   *  control byte instead of literal text — everything else (printable
   *  characters, IME composition, paste) is handled by `handleInput` below. */
  function handleKeydown(e: KeyboardEvent): void {
    if (e.ctrlKey && e.key.length === 1) {
      const code = e.key.toUpperCase().charCodeAt(0);
      if (code >= 65 && code <= 90) {
        e.preventDefault();
        sendBytes(new Uint8Array([code - 64])); // Ctrl+A..Z -> 0x01..0x1a
        return;
      }
    }
    switch (e.key) {
      case "Enter":
        e.preventDefault();
        sendBytes(new Uint8Array([0x0d]));
        break;
      case "Backspace":
        e.preventDefault();
        sendBytes(new Uint8Array([0x7f]));
        break;
      case "Tab":
        e.preventDefault();
        sendBytes(new Uint8Array([0x09]));
        break;
    }
  }

  /** Printable text lands here, not in `handleKeydown` — covers IME
   *  composition and paste for free. The field's value is never treated as
   *  terminal state (see the component doc comment), just cleared after
   *  forwarding it. */
  function handleInput(e: Event): void {
    const target = e.target as HTMLInputElement;
    const text = target.value;
    target.value = "";
    if (text) sendBytes(encoder.encode(text));
  }

  onMount(() => {
    void spawn();
  });

  onDestroy(() => {
    if (sessionId) void ctx.invoke("terminal_close", { id: sessionId });
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<!-- Clicking anywhere in the tile focuses the hidden `.typer` input below —
     the real keyboard target, already reachable on its own by Tab. -->
<div class="terminal" bind:this={root} onclick={() => inputEl?.focus()}>
  {#if error}
    <p class="error">{error}</p>
  {:else}
    <pre class="output" bind:this={outputEl}>{output}</pre>
    <input
      class="typer"
      bind:this={inputEl}
      type="text"
      autocomplete="off"
      autocapitalize="off"
      spellcheck="false"
      onkeydown={handleKeydown}
      oninput={handleInput}
    />
  {/if}
</div>

<style>
  .terminal {
    position: relative;
    height: 100%;
    background: var(--ax-surface-1);
    cursor: text;
  }
  .output {
    margin: 0;
    height: 100%;
    padding: var(--ax-space-2);
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-all;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
  }
  /* Captures keyboard focus/input without showing its own (always-empty)
   *  text — the shell's PTY echo in `.output` is the only visible typing
   *  feedback, same as a real terminal with local echo off. `pointer-events:
   *  none` is deliberate: without it this full-tile overlay would swallow
   *  mouse wheel/selection on `.output` underneath it (this project has a
   *  history of exactly that class of tile scroll bug) — `.terminal`'s own
   *  `onclick` still focuses it for keyboard capture regardless. */
  .typer {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    padding: 0;
    border: 0;
    background: transparent;
    color: transparent;
    caret-color: transparent;
    opacity: 0;
    pointer-events: none;
  }
  .error {
    padding: var(--ax-space-3);
    margin: 0;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-danger);
  }
</style>

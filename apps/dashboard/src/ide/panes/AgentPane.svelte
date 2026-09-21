<!--
  An agent in a pane: its harness running in a terminal, a status line saying
  which agent this is, and the side tab bar this milestone exists to build.

  Three things worth knowing before changing this:

  * **The terminal is the same `modules/terminal.svelte` everything else uses.**
    The agent is started by handing that module an opening command through its
    context (`initialCommand`), which it writes into the shell as if typed. So
    the harness runs *inside a shell*: when it exits, its output is still there
    and the pane is still usable, instead of going blank.
  * **Restart remounts the terminal.** `{#key restarts}` is the whole
    mechanism: the old component is destroyed, which closes its PTY session in
    `onDestroy`, and a new one spawns and types the command again. A "restart"
    that tried to reuse the session would have to reimplement everything the
    mount path already does.
  * **The side tab bar is built once and inhabited later.** "Terminal" is the
    only tab with content today; Plan arrives in CP6b, Diffs in M7.3 and Inbox
    in M7.5. They are shown, disabled, with what they are waiting for — a bar
    that grows tabs later would be a bar nobody laid out for four.
-->
<script lang="ts">
  import type { IdeAgent } from "../../core/backend";
  import { createContext } from "../../core/registry";
  import Terminal from "../../modules/terminal.svelte";

  let {
    agent,
    cwd,
    tabId,
  }: {
    agent: IdeAgent;
    /** Where the harness starts. CP5 makes this the agent's own worktree. */
    cwd: string;
    /** The dock tab this pane sits in — the terminal's `instanceId`. */
    tabId: string;
  } = $props();

  /** Bumped to remount the terminal, which is what a restart is. */
  let restarts = $state(0);

  type SideTab = { id: string; label: string; waiting?: string };
  const SIDE_TABS: SideTab[] = [
    { id: "terminal", label: "Terminal" },
    { id: "plan", label: "Plan", waiting: "Arrives with the plan tab (CP6b)" },
    { id: "diffs", label: "Diffs", waiting: "Arrives with the git layer (M7.3)" },
    { id: "inbox", label: "Inbox", waiting: "Arrives with agent-to-agent messaging (M7.5)" },
  ];
  let sideTab = $state("terminal");

  const command = $derived(agent.effective_command);

  /**
   * A context per terminal mount.
   *
   * Rebuilt on every restart on purpose: it carries the working directory and
   * the profile's environment, and a restart is exactly the moment to pick up
   * a profile edit. Config changes are dropped — a pane's terminal settings
   * are global (Checkpoint 5d) and what is in here is derived from the agent
   * row, so writing it back would only fight the next restart.
   *
   * The opening command is **not** in here: it goes to the terminal as a prop,
   * so that no stored layout can ever be a source for it. See that prop's own
   * documentation in `modules/terminal.svelte`.
   */
  const terminalContext = $derived.by(() => {
    // Read `restarts` so a remount really does build a fresh context.
    void restarts;
    return createContext(
      `${tabId}:${restarts}`,
      { cwd, env: agent.env },
      // Config changes go nowhere on purpose: everything in this context is
      // derived from the agent row, so storing a change on the tab would only
      // be overwritten by the next restart. The agent profile is the truth.
      () => {},
    );
  });
</script>

<div class="agent-pane">
  <div class="body">
    <!-- The terminal is always mounted, and hidden when another side tab is
         showing — never behind an `{#if}`. Unmounting it closes the PTY and
         restarts the agent, which is exactly what looking at the Plan tab for
         a moment must not do (owner report). Same rule, and the same
         `visibility`-not-`display` reason, as an inactive dock tab. -->
    <div
      class="terminal-slot"
      class:hidden={sideTab !== "terminal"}
      inert={sideTab !== "terminal"}
      id="agent-view-terminal"
      role="tabpanel"
      aria-labelledby="agent-tab-terminal"
    >
      {#key restarts}
        <Terminal ctx={terminalContext} initialCommand={command} />
      {/key}
    </div>

    {#if sideTab !== "terminal"}
      {@const tab = SIDE_TABS.find((t) => t.id === sideTab)}
      <div class="waiting" id="agent-view-{sideTab}" role="tabpanel" aria-labelledby="agent-tab-{sideTab}">
        <p>{tab?.waiting}</p>
      </div>
    {/if}
  </div>

  <div class="side" role="tablist" aria-orientation="vertical" aria-label="{agent.name} views">
    {#each SIDE_TABS as tab (tab.id)}
      <button
        type="button"
        role="tab"
        class="side-tab"
        class:active={sideTab === tab.id}
        id="agent-tab-{tab.id}"
        aria-selected={sideTab === tab.id}
        aria-controls="agent-view-{tab.id}"
        title={tab.waiting ?? tab.label}
        onclick={() => (sideTab = tab.id)}>{tab.label}</button
      >
    {/each}
  </div>

  <footer class="status">
    <span class="name">{agent.name}</span>
    <span class="harness">{agent.harness}</span>
    {#if agent.model}<span class="model">{agent.model}</span>{/if}
    <code class="command" title={command}>{command}</code>
    <button type="button" class="restart" onclick={() => (restarts += 1)}>Restart</button>
  </footer>
</div>

<style>
  .agent-pane {
    position: absolute;
    inset: 0;
    display: grid;
    /* Content beside the side bar, status line under both. */
    grid-template-columns: 1fr auto;
    grid-template-rows: 1fr auto;
    grid-template-areas:
      "body side"
      "status status";
    min-width: 0;
    min-height: 0;
  }

  .body {
    grid-area: body;
    position: relative;
    min-width: 0;
    min-height: 0;
  }

  .terminal-slot {
    position: absolute;
    inset: 0;
  }

  /* Not `display: none`: a hidden terminal keeps its measured size, so it does
     not tell its PTY it has zero rows while a sibling tab is on screen. */
  .terminal-slot.hidden {
    visibility: hidden;
    pointer-events: none;
  }

  .waiting {
    position: absolute;
    inset: 0;
    padding: var(--ax-space-4);
    background: var(--ax-surface-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  .waiting p {
    margin: 0;
  }

  .side {
    grid-area: side;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1);
    border-left: 1px solid var(--ax-border);
    background: var(--ax-surface-2);
  }

  .side-tab {
    /* Reads bottom-to-top along the right edge, like an editor's side bar. */
    writing-mode: vertical-rl;
    transform: rotate(180deg);
    padding: var(--ax-space-2) var(--ax-space-1);
    background: none;
    border: none;
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    letter-spacing: var(--ax-tracking-wide);
    cursor: pointer;
  }

  .side-tab:hover {
    color: var(--ax-text);
    background: var(--ax-surface-3);
  }

  .side-tab.active {
    color: var(--ax-accent);
    background: var(--ax-surface-1);
  }

  .side-tab:focus-visible {
    outline: var(--ax-focus-ring);
  }

  .status {
    grid-area: status;
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
    padding: var(--ax-space-1) var(--ax-space-3);
    border-top: 1px solid var(--ax-border);
    background: var(--ax-surface-2);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
    white-space: nowrap;
  }

  .name {
    color: var(--ax-text);
  }

  .harness,
  .model {
    padding: 0 var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
  }

  .command {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: var(--ax-font-mono);
  }

  .restart {
    padding: 0 var(--ax-space-2);
    background: var(--ax-surface-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  .restart:hover {
    color: var(--ax-accent);
    border-color: var(--ax-accent);
  }
</style>

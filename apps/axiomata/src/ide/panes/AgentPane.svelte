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
  * **The side tab bar is built once and inhabited later.** Terminal and Plan
    (CP6b) have content; Diffs arrives in M7.3 and Inbox in M7.5. They are
    shown with what they are waiting for — a bar that grows tabs later would be
    a bar nobody laid out for four.
  * **Status and plan come from the harness, not the screen** (CP6/CP6b):
    `ide/agentStatus.ts` polls the agent's status channel, and the command and
    environment this pane starts with are the ones `prepare_ide_agent` built
    to connect it — `launch_command` / `launch_env`, never the profile's own.
-->
<script lang="ts">
  import type { IdeAgent, ProvisionedAgent } from "../../core/backend";
  import AgentTeamEntry from "../AgentTeamEntry.svelte";
  import { ownerIsQuiet, shouldAsk } from "../nudge";
  import { createContext } from "../../core/registry";
  import { toast } from "../../core/toast";
  import type { OutputRef } from "../../core/outputLinks";
  import { relativeInside } from "../outputPath";
  import { renderMarkdown } from "../../core/markdown";
  import Terminal from "../../modules/terminal.svelte";
  import { mailboxNudge, mailboxNudged, newAgentSession, prepareAgent } from "../agents";
  import { agentStatus, describeStatus } from "../agentStatus";
  import DiffView from "../DiffView.svelte";
  import Icon from "../../ui/Icon.svelte";
  import IconButton from "../../ui/IconButton.svelte";
  import type { IconName } from "../../ui/icons/lucide";
  import { getDock } from "../dockContext";
  import { agentDiffOf, agentDiffTab } from "../paneKinds";
  import { openFileBeside } from "./openFile";
  import StatusDot from "../StatusDot.svelte";

  let {
    agent,
    cwd,
    tabId,
    visible = true,
  }: {
    agent: IdeAgent;
    /** The project folder — where the harness runs when the project is not a
     *  git repository, and what is shown until the worktree is ready. */
    cwd: string;
    /** The dock tab this pane sits in — the terminal's `instanceId`. */
    tabId: string;
    /** The pane is on screen (the Diffs tab only polls then, G4). */
    visible?: boolean;
  } = $props();

  /** Bumped to remount the terminal, which is what a restart is. */
  let restarts = $state(0);

  /**
   * The worktree and port, fetched before the harness starts.
   *
   * The terminal waits for this rather than starting in the project folder and
   * being moved later: a shell cannot change the directory it was born in, and
   * an agent that started in the wrong place would quietly edit the user's own
   * working copy instead of its branch. Re-fetched on restart, so a repaired
   * or recreated worktree is picked up.
   */
  let ready = $state<ProvisionedAgent | null>(null);
  let failure = $state<string | null>(null);

  $effect(() => {
    const id = agent.id;
    void restarts;
    ready = null;
    failure = null;
    prepareAgent(id)
      .then((provisioned) => {
        if (agent.id === id) ready = provisioned;
      })
      .catch((err: unknown) => {
        const message = err instanceof Error ? err.message : String(err);
        if (agent.id === id) failure = message;
        toast(`${agent.name}: ${message}`, "danger");
      });
  });

  type SideTab = { id: string; label: string; icon: IconName; waiting?: string };
  // An icon rail, as editors have (editor-look I2); the label is the tooltip.
  const SIDE_TABS: SideTab[] = [
    { id: "terminal", label: "Terminal", icon: "terminal" },
    { id: "plan", label: "Plan", icon: "list-checks" },
    { id: "diffs", label: "Diffs", icon: "git-compare" },
    { id: "inbox", label: "Inbox", icon: "inbox" },
  ];
  /** How many files the agent changed, once the Diffs view has looked (its badge). */
  let changedFiles = $state<number | null>(null);
  let sideTab = $state("terminal");
  const dock = getDock();
  const openFile = openFileBeside(() => tabId);

  /** A `file:line` ⌘-clicked in the agent's output: opened from its worktree (or the project folder, which is where it runs without one). */
  function openOutputLink(ref: OutputRef): void {
    const rel = relativeInside(ref.path, ready?.cwd ?? cwd);
    if (!rel) {
      toast(`“${ref.path}” is not inside this agent's folder.`, "warning");
      return;
    }
    openFile(agent.id, rel, ref.line - 1);
  }

  /** The agent's diffs in a dock pane of their own (H14) — at most one per agent. */
  function dockDiffs(): void {
    dock.open(agentDiffTab(agent.id, agent.name), (t) => agentDiffOf(t) === agent.id, tabId);
  }

  /** The Diffs view stays mounted once opened, so its folds and file survive a tab switch. */
  let diffsOpened = $state(false);
  $effect(() => {
    if (sideTab === "diffs") diffsOpened = true;
  });

  /** What is typed into the shell — including the status hookup. */
  const command = $derived(ready?.launch_command ?? agent.effective_command);

  /**
   * An Opencode agent on the generated command runs in a session the IDE
   * keeps for it (opencode2.md OC2); a restart continues it. "New session"
   * forgets it, so the restart that follows opens a fresh one — the old one
   * stays in Opencode's own list.
   */
  const keepsSession = $derived(agent.harness === "opencode" && !agent.command.trim());

  async function startNewSession(): Promise<void> {
    try {
      await newAgentSession(agent.id);
      restarts += 1;
    } catch (err) {
      toast(`${agent.name}: ${err instanceof Error ? err.message : String(err)}`, "danger");
    }
  }

  const statuses = agentStatus.statuses;
  const status = $derived($statuses.byAgent.get(agent.id));

  /** The terminal's two hands the nudge needs (`modules/terminal.svelte`). */
  let terminal = $state<{ msSinceInput: () => number | null; typeLine: (line: string) => Promise<boolean> } | null>(
    null,
  );
  let nudgeBusy = false;
  let lastAskedAt: number | null = null;

  // Mail for an agent that is waiting for its next prompt (A2A A8, way 2): on every status tick, ask whether there is
  // a line to type — `nudge.ts` holds the rules (idle only, never while the owner types, not more than every few
  // seconds).
  $effect(() => {
    void $statuses.checkedAt;
    const state = status?.state;
    if (!ready || !terminal || !state) return;
    const now = performance.now();
    const situation = {
      state,
      sinceOwnInputMs: terminal.msSinceInput(),
      sinceAskMs: lastAskedAt === null ? null : now - lastAskedAt,
      busy: nudgeBusy,
    };
    if (!shouldAsk(situation)) return;
    lastAskedAt = now;
    nudgeBusy = true;
    void announceMail(state).finally(() => (nudgeBusy = false));
  });

  async function announceMail(state: string): Promise<void> {
    try {
      const line = await mailboxNudge(agent.id, state);
      // Things moved while the backend was asked: look again, now, before anything is typed.
      if (!line || status?.state !== "idle" || !terminal || !ownerIsQuiet(terminal.msSinceInput())) return;
      if (await terminal.typeLine(line)) await mailboxNudged(agent.id);
    } catch {
      // A nudge is a courtesy: the agent reads its inbox itself (way 1), so a failure here loses nothing.
    }
  }
  // `checkedAt` moves every tick, which is what lets the "own command stayed
  // silent" rule change its mind without a clock of its own.
  const statusView = $derived(describeStatus(status, agent, $statuses.checkedAt));
  const plan = $derived(status?.plan ?? null);
  const planDocument = $derived(status?.plan_document ?? null);
  /** Done or dropped — either way no longer ahead of the agent. */
  const finishedSteps = $derived(
    plan?.steps.filter((s) => s.state === "done" || s.state === "cancelled").length ?? 0,
  );

  const STEP_MARK = { todo: "○", doing: "▸", done: "✓", cancelled: "✗" } as const;

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
      // The worktree, and the env that goes with it. Both come from Rust —
      // `launch_env` carries the identity (AXIOMATA_AGENT_ID and friends) and
      // Claude Code's status channel (AXIOMATA_EVENTS, AXIOMATA_CLAUDE_SETTINGS, …).
      { cwd: ready?.cwd ?? cwd, env: ready?.launch_env ?? agent.effective_env },
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
      {#if ready}
        {#key restarts}
          <Terminal bind:this={terminal} ctx={terminalContext} initialCommand={command} onLink={openOutputLink} />
        {/key}
      {:else if failure}
        <p class="pending error">{failure}</p>
      {:else}
        <p class="pending">Preparing this agent's worktree…</p>
      {/if}
    </div>

    <!-- Like the terminal: hidden, not unmounted, once it has been opened. -->
    {#if diffsOpened}
      <div
        class="diffs-slot"
        class:hidden={sideTab !== "diffs"}
        inert={sideTab !== "diffs"}
        id="agent-view-diffs"
        role="tabpanel"
        aria-labelledby="agent-tab-diffs"
      >
        <DiffView
          {agent}
          agentState={status?.state ?? null}
          visible={visible && sideTab === "diffs"}
          place="tab"
          onOpenFile={(rel, line) => openFile(agent.id, rel, line)}
          onDock={dockDiffs}
          onCount={(n) => (changedFiles = n)}
        />
      </div>
    {/if}

    {#if sideTab === "plan"}
      <div class="plan" id="agent-view-plan" role="tabpanel" aria-labelledby="agent-tab-plan">
        {#if plan || planDocument}
          <!-- The tasks are the live part — what the agent is doing now — so
               they sit in their own framed block with a count, above the
               plan-mode document, and say so when there are none yet
               (owner, live test: "hard to tell whether there was a task"). -->
          <section class="tasks" class:stale={plan?.from_earlier_session} aria-label="Tasks">
            <header>
              <h3>Tasks</h3>
              {#if plan}
                <span class="count">{finishedSteps} / {plan.steps.length} done</span>
              {/if}
            </header>
            {#if plan}
              <div class="progress" aria-hidden="true">
                <div class="fill" style:width="{(finishedSteps / plan.steps.length) * 100}%"></div>
              </div>
              {#if plan.from_earlier_session}
                <p class="note">From an earlier session — the agent has not written new tasks since it started.</p>
              {/if}
              <ol>
                {#each plan.steps as step, index (index)}
                  <li class="step {step.state}" title={step.detail ?? undefined}>
                    <span class="mark" aria-hidden="true">{STEP_MARK[step.state]}</span>
                    <span class="text">{step.text}</span>
                  </li>
                {/each}
              </ol>
            {:else}
              <p class="note">No tasks yet — they appear once the agent starts working on the plan below.</p>
            {/if}
          </section>
          {#if planDocument}
            <!-- Open while there is no task list yet, so a fresh plan-mode
                 plan is what you see; folded away once the tasks carry the
                 progress. Sanitised by `renderMarkdown` (DOMPurify): the text
                 comes from the agent. -->
            <details class="document" class:stale={planDocument.from_earlier_session} open={!plan}>
              <summary>
                Plan (plan mode) · {planDocument.name}{planDocument.from_earlier_session ? " · earlier session" : ""}
              </summary>
              <div class="md">{@html renderMarkdown(planDocument.markdown)}</div>
            </details>
          {/if}
        {:else if statusView.tone === "none"}
          <p class="note">{statusView.title}</p>
        {:else}
          <p class="note">No plan yet. It appears here as soon as the agent writes one.</p>
        {/if}
      </div>
    {:else if sideTab === "inbox"}
      <div class="inbox" id="agent-view-inbox" role="tabpanel" aria-labelledby="agent-tab-inbox">
        <AgentTeamEntry entry={ready?.mcp ?? null} />
      </div>
    {:else if sideTab !== "terminal" && sideTab !== "diffs"}
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
        aria-label={tab.label}
        onclick={() => (sideTab = tab.id)}
      >
        <Icon name={tab.icon} />
        {#if tab.id === "diffs" && changedFiles}<span class="badge">{changedFiles}</span>{/if}
      </button>
    {/each}
  </div>

  <footer class="status">
    <span class="state" title={statusView.title}>
      <StatusDot view={statusView} />
      {statusView.label}
    </span>
    <span class="name">{agent.name}</span>
    <span class="harness">{agent.harness}{agent.model ? ` · ${agent.model}` : ""}</span>
    {#if ready?.shared_folder}
      <span class="branch" title="The project is not a git repository, so agents share its folder">
        shared folder
      </span>
    {:else if ready?.agent.branch}
      <span class="branch" title={ready.cwd}>{ready.agent.branch}</span>
    {/if}
    {#if ready?.agent.port}<span class="port" title="AXIOMATA_PORT">:{ready.agent.port}</span>{/if}
    <code class="command" title={command}>{command}</code>
    {#if keepsSession}
      <IconButton
        icon="message-square-plus"
        size="sm"
        label={ready?.agent.opencode_session
          ? `New session — leave ${ready.agent.opencode_session} and start a fresh one`
          : "New session — start a fresh Opencode session"}
        onclick={startNewSession}
      />
    {/if}
    <IconButton icon="rotate-ccw" size="sm" label="Restart the agent" onclick={() => (restarts += 1)} />
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

  .diffs-slot {
    position: absolute;
    inset: 0;
  }

  .diffs-slot.hidden {
    display: none;
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

  .inbox {
    position: absolute;
    inset: 0;
    overflow-y: auto;
    padding: var(--ax-space-4);
    background: var(--ax-surface-1);
    color: var(--ax-text);
    font-size: var(--ax-font-size-sm);
  }

  .plan {
    position: absolute;
    inset: 0;
    overflow-y: auto;
    padding: var(--ax-space-4);
    background: var(--ax-surface-1);
    color: var(--ax-text);
    font-size: var(--ax-font-size-sm);
  }

  .plan .note {
    margin: 0 0 var(--ax-space-3);
    color: var(--ax-text-muted);
  }

  .plan ol {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .tasks.stale,
  .document.stale {
    opacity: 0.6;
  }

  .tasks {
    padding: var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-left: 2px solid var(--ax-accent);
    border-radius: var(--ax-radius-md);
  }

  .tasks header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: var(--ax-space-2);
  }

  .tasks h3 {
    margin: 0;
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text);
  }

  .tasks .count {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .tasks .note {
    margin: 0 0 var(--ax-space-2);
  }

  .tasks .note:last-child {
    margin-bottom: 0;
  }

  .progress {
    height: var(--ax-space-1);
    margin-bottom: var(--ax-space-3);
    background: var(--ax-surface-3);
    border-radius: var(--ax-radius-pill);
    overflow: hidden;
  }

  .progress .fill {
    height: 100%;
    background: var(--ax-accent);
    transition: width var(--ax-dur-med) var(--ax-ease);
  }

  .document {
    margin-top: var(--ax-space-4);
    padding-top: var(--ax-space-3);
    border-top: 1px solid var(--ax-border);
  }

  .document summary {
    color: var(--ax-text-muted);
    cursor: pointer;
  }

  .document .md {
    margin-top: var(--ax-space-2);
  }

  .document .md :global(h1),
  .document .md :global(h2),
  .document .md :global(h3) {
    margin: var(--ax-space-3) 0 var(--ax-space-2);
    font-size: var(--ax-font-size-base);
  }

  .document .md :global(p),
  .document .md :global(ul),
  .document .md :global(ol),
  .document .md :global(pre) {
    margin: 0 0 var(--ax-space-2);
  }

  .document .md :global(code) {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    background: var(--ax-surface-3);
    padding: 0 var(--ax-space-1);
    border-radius: var(--ax-radius-sm);
  }

  .step {
    display: flex;
    gap: var(--ax-space-2);
    align-items: baseline;
  }

  .step .mark {
    flex: 0 0 auto;
    width: var(--ax-space-4);
    color: var(--ax-text-muted);
    font-family: var(--ax-font-mono);
  }

  .step.doing .mark,
  .step.doing .text {
    color: var(--ax-accent);
  }

  .step.done .mark {
    color: var(--ax-success);
  }

  .step.done .text,
  .step.cancelled .text {
    color: var(--ax-text-muted);
  }

  .step.cancelled .text {
    text-decoration: line-through;
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
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    width: var(--ax-hit-min);
    height: var(--ax-hit-min);
    padding: 0;
    background: none;
    border: none;
    border-radius: var(--ax-radius-md);
    color: var(--ax-text-muted);
    cursor: pointer;
  }

  .side-tab:hover {
    color: var(--ax-text);
    background: var(--ax-surface-3);
  }

  .side-tab.active {
    color: var(--ax-accent);
    background: var(--ax-accent-muted);
  }

  .side-tab:focus-visible {
    outline: var(--ax-focus-ring);
  }

  /* The count of changed files, on the Diffs icon's corner. */
  .badge {
    position: absolute;
    top: 0;
    right: 0;
    min-width: calc(14px * var(--ax-ui-scale));
    padding: 0 3px;
    border-radius: var(--ax-radius-pill);
    background: var(--ax-accent);
    color: var(--ax-text-invert);
    font-family: var(--ax-font-sans);
    font-size: calc(10px * var(--ax-ui-scale));
    line-height: calc(14px * var(--ax-ui-scale));
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

  .state {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
  }

  .pending {
    position: absolute;
    inset: 0;
    margin: 0;
    padding: var(--ax-space-4);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  .pending.error {
    color: var(--ax-danger);
  }

  .command {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    font-family: var(--ax-font-mono);
  }

</style>

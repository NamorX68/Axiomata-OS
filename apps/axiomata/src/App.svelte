<!-- Shell composition: Dock → Canvas → AssistantBar, plus overlays. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";

  import Canvas from "./canvas/Canvas.svelte";
  import Ground from "./ui/Ground.svelte";
  import {
    type ClaudeLoginStatus,
    type ConfigView,
    invokeBackend,
    listenBackend,
    type PlanRunEvent,
  } from "./core/backend";
  import { claudeSignInNotice } from "./core/harness";
  import { AGENTS_WAITING, watchWaiting } from "./core/agentWaiting";
  import { brainActive } from "./core/brainView";
  import { closeTop } from "./core/closeStack";
import { emit, on } from "./core/bus";
  import { openFilePanel, openNewNote, openStaged, requestClose, staged } from "./core/staging";
  import { globalKeyAction } from "./core/globalKeys";
  import { inTerminalPane, platform } from "./core/keys";
  import { loadInstances } from "./core/stores";
  import { toast } from "./core/toast";
  import { openKanban } from "./modules/kanbanApp";
  import AssistantBar from "./shell/AssistantBar.svelte";
  import ChatPanel from "./shell/ChatPanel.svelte";
  import { requestAgent } from "./ide/agentRequest";
  import { endedSessions, runEventNote } from "./ide/planning";
  import { requestMode } from "./ide/modeRequest";
  import IdeView from "./ide/IdeView.svelte";
  import ModulePicker from "./shell/ModulePicker.svelte";
  import SecondBrainView from "./shell/SecondBrainView.svelte";
  import Settings from "./shell/Settings.svelte";
  import HandbookPanel from "./shell/HandbookPanel.svelte";
  import { HANDBOOK_EVENT } from "./core/handbook";
  import Spotlight from "./shell/Spotlight.svelte";
  import StagingLayer from "./shell/StagingLayer.svelte";
  import Toasts from "./shell/Toasts.svelte";
  import Ambient from "./shell/Ambient.svelte";
  import Dock from "./shell/Dock.svelte";
  import Ignition from "./shell/Ignition.svelte";
  import Sounds from "./shell/Sounds.svelte";
  import { focusMode, toggleFocus } from "./core/focusMode";

  let pickerOpen = $state(false);
  let settingsOpen = $state(false);
  /** The settings tab asked for with `shell:settings` (`{ tab }`), else the one last shown. */
  let settingsTab = $state<string | null>(null);
  // The IDE is mounted from the first time it is opened and never unmounted
  // again — it hides itself instead. Every pane in it would otherwise be
  // destroyed on the way back to the dashboard, and a terminal pane closes its
  // PTY session when destroyed, so a glance at the canvas would kill every
  // running shell. `ideStarted` is what keeps it out of the tree until it is
  // wanted at all; `ideOpen` is what it listens to afterwards.
  let ideStarted = $state(false);
  let ideOpen = $state(false);
  let spotlightOpen = $state(false);
  let handbookOpen = $state(false);
  /** The page asked for with `shell:handbook` (`{ page }`), numbered so the same page asked for again is shown. */
  let handbookRequest = $state<{ page: string | null; seq: number } | null>(null);
  let brainOpen = $state(false);
  let brainFocus = $state<string | null>(null);
  /** The Second Brain's filter asked for from outside, numbered so the same text asked for twice is set again. */
  let brainQuery = $state<{ text: string; seq: number } | null>(null);

  /**
   * The window-wide keys (`core/globalKeys.ts`): ⌘K toggles the spotlight from every view, ⌘, opens the settings and ⌘W
   * closes the topmost thing — the spotlight, else the newest dialog (`core/closeStack.ts`), else the top staged panel
   * (a panel that answers ⌘W itself, like the file window, has marked the key as handled). Not while a terminal pane
   * has the focus: ⌘W there is nobody's business.
   */
  function onWindowKeydown(event: KeyboardEvent): void {
    const inTerminal = inTerminalPane(event);
    const action = globalKeyAction(event, get(platform), inTerminal);
    if (!action || event.defaultPrevented) return;
    if (action === "close") {
      if (inTerminal) return;
      if (spotlightOpen) {
        event.preventDefault();
        spotlightOpen = false;
        return;
      }
      // A dialog (settings, a picker, the chat) sits above the staged panels.
      if (closeTop()) {
        event.preventDefault();
        return;
      }
      const open = get(staged);
      const top = open[open.length - 1];
      if (!top) return;
      event.preventDefault();
      void requestClose(top.id);
      return;
    }
    event.preventDefault();
    if (action === "spotlight") spotlightOpen = !spotlightOpen;
    else if (action === "focus") toggleFocus();
    else settingsOpen = true;
  }

  // The focus mode (G13) is a mark on the document: `styles.css` lets everything but the work in front recede.
  $effect(() => {
    if ($focusMode) document.documentElement.dataset.focus = "on";
    else delete document.documentElement.dataset.focus;
  });

  // The Orbit's own click handler needs to know whether the layer is up (`core/brainView.ts`).
  $effect(() => brainActive.set(brainOpen));

  onMount(() => {
    // A role on Claude Code without its sign-in would fail every turn: say so once at start (harness-choice D7).
    void claudeSignInNotice(
      () => invokeBackend<ConfigView>("get_config"),
      () => invokeBackend<ClaudeLoginStatus>("claude_login_status"),
    ).then((notice) => {
      if (notice) toast(notice, "warning");
    });
    const offs = [
      on("shell:add-module", () => (pickerOpen = true)),
      // A file handed over by a module, the chat or the agent → staged viewer.
      on("open-file", (detail) => {
        const d = (detail ?? {}) as { path?: string; mode?: string };
        if (typeof d.path !== "string") return;
        openFilePanel(d.path, d.mode === "edit" ? "edit" : "read");
      }),
      on("shell:settings", (detail) => {
        const d = (detail ?? {}) as { tab?: string };
        settingsTab = typeof d.tab === "string" ? d.tab : null;
        settingsOpen = true;
      }),
      // The handbook (chat-knows-app CH4): the Dock's `?`, a spotlight hit, `/help <topic>`, the chat's open action.
      on(HANDBOOK_EVENT, (detail) => {
        const d = (detail ?? {}) as { page?: string };
        handbookRequest = { page: typeof d.page === "string" ? d.page : null, seq: (handbookRequest?.seq ?? 0) + 1 };
        handbookOpen = true;
      }),
      // The ring's "Kanban" entry: the board opens as a large panel (Kanban is an app, it has no tile).
      on("shell:kanban", () => void openKanban()),
      // The ring's single "Studio" entry opens the workbench in the mode it was left in.
      on("shell:studio", () => {
        ideStarted = true;
        ideOpen = true;
      }),
      // The old entry points: "IDE" shows it in the Agents mode, "Editor" (and a panel hand-over) in the Editor mode. "IDE" shows it in the Agents mode, "Editor" in the Editor mode.
      on("shell:ide", () => {
        requestMode("agents");
        ideStarted = true;
        ideOpen = true;
      }),
      // A card was started: its session opens in the Studio (A2A CP-A6a).
      on("shell:agent", (detail) => {
        const d = (detail ?? {}) as { projectId?: number; agentId?: number };
        if (typeof d.projectId !== "number" || typeof d.agentId !== "number") return;
        requestAgent({ projectId: d.projectId, agentId: d.agentId });
        ideStarted = true;
        ideOpen = true;
      }),
      on("shell:editor", () => {
        requestMode("editor");
        ideStarted = true;
        ideOpen = true;
      }),
      // The top-bar search icon and ⌘K: the spotlight (the Second Brain keeps its own way in: the cloud, `/brain`).
      on("shell:spotlight", () => (spotlightOpen = !spotlightOpen)),
      // Reuses the Document module's own compose mode instead of a bespoke
      // dialog — same viewer, same Save-picks-the-folder agent flow.
      on("shell:new-note", () => openNewNote()),
      // A click on the Orbit's empty background closes the Second Brain layer again.
      on("close-second-brain", () => (brainOpen = false)),
      // The background graph (or /brain) → the Second Brain layer over the cloud.
      on("open-second-brain", (detail) => {
        const d = (detail ?? {}) as { focus?: string | null; query?: string };
        brainFocus = typeof d.focus === "string" ? d.focus : null;
        // Only an asked-for filter is handed on: a plain open must not bring back the last one.
        brainQuery = typeof d.query === "string" ? { text: d.query, seq: (brainQuery?.seq ?? 0) + 1 } : null;
        brainOpen = true;
      }),
    ];
    if (import.meta.env.DEV) {
      // Browser-console handle for driving the shell without Tauri.
      void import("./core/devmock").then((m) => {
        (window as unknown as { __ax: unknown }).__ax = {
          emit,
          openStaged,
          loadInstances,
          setMockCustomCss: m.setMockCustomCss,
          mockExternalWrite: m.mockExternalWrite,
          mockPickNext: m.mockPickNext,
          mockAgentState: m.mockAgentState,
          mockEmit: m.mockEmit,
        };
      });
    }
    // A reviewer was made for a reported card (A2A CP-A6b): its pane opens in the Studio, which stays where it is on
    // screen — the pane is what starts the harness, so it has to exist, but the owner is not pulled out of what they do.
    const review = [
      listenBackend<{ cardId: number; projectId: number; agentId: number }>("card:review-started", (started) => {
        requestAgent({ projectId: started.projectId, agentId: started.agentId, background: true });
        ideStarted = true;
        toast(`Card #${started.cardId}: a reviewer is checking it (Studio, Agents).`, "info");
      }),
      listenBackend<{ cardId: number; reason: string }>("card:review-blocked", (blocked) => {
        toast(`Card #${blocked.cardId} waits for a review: ${blocked.reason}`, "warning");
      }),
      // A plan that runs by itself did something (A2A CP-A8): a started card's pane opens in the background — opening it
      // is what starts the harness —, the panes of integrated cards close, and what needs the owner is said.
      listenBackend<PlanRunEvent>("plan:run", (event) => {
        if (event.event === "started") {
          requestAgent({ projectId: event.project_id, agentId: event.agent_id, background: true });
          ideStarted = true;
        }
        const ended = endedSessions(event);
        if (ended.length > 0) emit("studio:close-agent-panes", { agentIds: ended });
        // An escalated card keeps its session: the pane that ran the old engine is closed above and opened again here.
        if (event.event === "escalated") {
          requestAgent({ projectId: event.project_id, agentId: event.agent_id, background: true });
          ideStarted = true;
        }
        const note = runEventNote(event);
        if (note) toast(note.text, note.tone);
      }),
      // A card session used up a limit and was stopped (A2A CP-A6c): the card keeps it, the owner decides.
      listenBackend<{ cardId: number | null; planId: number | null; agentName: string; reason: string }>(
        "card:limit-reached",
        (stopped) => {
          const what = stopped.cardId !== null ? `Card #${stopped.cardId}` : `Plan #${stopped.planId}`;
          toast(`${what}: ${stopped.agentName} was stopped at the limit (${stopped.reason}).`, "warning");
        },
      ),
    ];
    // Card sessions (workers and reviewers) made before the page was listening (or before a reload) still need their
    // panes: asked once, after the listeners are in, so none is lost between the two.
    void Promise.all(review).then(async () => {
      try {
        const open = await invokeBackend<{ card_id: number; project_id: number; agent_id: number }[]>(
          "open_card_sessions",
        );
        for (const session of open) {
          // Background: the owner's project and mode stay as they are.
          requestAgent({ projectId: session.project_id, agentId: session.agent_id, background: true });
          ideStarted = true;
        }
      } catch {
        // Without the list the watcher's events still arrive; a session's pane can be opened from the Agents list.
      }
    });
    // Which agents wait for the owner, in any project: the badge in the Dock, over the Orbit and on the Kanban's cards.
    const waiting = watchWaiting(
      () => invokeBackend("waiting_agents"),
      (handler) => listenBackend(AGENTS_WAITING, handler),
    ).catch(() => () => {});
    return () => {
      offs.forEach((off) => off());
      review.forEach((pending) => void pending.then((off) => off()));
      void waiting.then((off) => off());
    };
  });
</script>

<svelte:window onkeydown={onWindowKeydown} />

<Ground fixed />
<Ambient />
<Dock />
<Ignition />
<Sounds />
<Toasts />
<ModulePicker bind:open={pickerOpen} />
<Settings bind:open={settingsOpen} initialTab={settingsTab} />
<Spotlight bind:open={spotlightOpen} />
<HandbookPanel bind:open={handbookOpen} request={handbookRequest} />
{#if brainOpen}
  <SecondBrainView bind:open={brainOpen} focus={brainFocus} query={brainQuery} />
{/if}
{#if ideStarted}
  <IdeView bind:open={ideOpen} />
{/if}
<StagingLayer />
<ChatPanel />

<Canvas />
<!-- The assistant bar floats over the bottom of the screen, which is fine over
     the canvas or the Second Brain but sits squarely on an agent pane's status
     line in the IDE. Hidden rather than unmounted so a
     half-typed prompt is still there when the view is closed again. -->
<div class="assistant-host" class:hidden={ideOpen}>
  <AssistantBar />
</div>

<style>
  .assistant-host.hidden {
    display: none;
  }
</style>

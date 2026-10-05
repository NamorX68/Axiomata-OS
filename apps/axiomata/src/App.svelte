<!-- Shell composition: TopBar → IconBar → Canvas → AssistantBar, plus overlays. -->
<script lang="ts">
  import { onMount } from "svelte";

  import Canvas from "./canvas/Canvas.svelte";
  import { invokeBackend, listenBackend, type PlanRunEvent } from "./core/backend";
  import { emit, on } from "./core/bus";
  import { openFilePanel, openNewNote, openStaged } from "./core/staging";
  import { loadInstances } from "./core/stores";
  import { toast } from "./core/toast";
  import { openKanban } from "./modules/kanbanApp";
  import AssistantBar from "./shell/AssistantBar.svelte";
  import ChatPanel from "./shell/ChatPanel.svelte";
  import { requestAgent } from "./ide/agentRequest";
  import { endedSessions, runEventNote } from "./ide/planning";
  import { requestMode } from "./ide/modeRequest";
  import IdeView from "./ide/IdeView.svelte";
  import IconBar from "./shell/IconBar.svelte";
  import ModulePicker from "./shell/ModulePicker.svelte";
  import SecondBrainView from "./shell/SecondBrainView.svelte";
  import Settings from "./shell/Settings.svelte";
  import StagingLayer from "./shell/StagingLayer.svelte";
  import Toasts from "./shell/Toasts.svelte";
  import TopBar from "./shell/TopBar.svelte";

  let pickerOpen = $state(false);
  let settingsOpen = $state(false);
  // The IDE is mounted from the first time it is opened and never unmounted
  // again — it hides itself instead. Every pane in it would otherwise be
  // destroyed on the way back to the dashboard, and a terminal pane closes its
  // PTY session when destroyed, so a glance at the canvas would kill every
  // running shell. `ideStarted` is what keeps it out of the tree until it is
  // wanted at all; `ideOpen` is what it listens to afterwards.
  let ideStarted = $state(false);
  let ideOpen = $state(false);
  let brainOpen = $state(false);
  let brainFocus = $state<string | null>(null);
  let brainQuery = $state("");

  onMount(() => {
    const offs = [
      on("shell:add-module", () => (pickerOpen = true)),
      // A file handed over by a module, the chat or the agent → staged viewer.
      on("open-file", (detail) => {
        const d = (detail ?? {}) as { path?: string; mode?: string };
        if (typeof d.path !== "string") return;
        openFilePanel(d.path, d.mode === "edit" ? "edit" : "read");
      }),
      on("shell:settings", () => (settingsOpen = true)),
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
      // The top-bar search icon → the Second Brain, focused on its search
      // box (SecondBrainView autofocuses when opened with no query/target).
      on("shell:search", () => {
        brainFocus = null;
        brainQuery = "";
        brainOpen = true;
      }),
      // Reuses the Document module's own compose mode instead of a bespoke
      // dialog — same viewer, same Save-picks-the-folder agent flow.
      on("shell:new-note", () => openNewNote()),
      // The background graph (or /brain) → full-screen Second Brain.
      on("open-second-brain", (detail) => {
        const d = (detail ?? {}) as { focus?: string | null; query?: string };
        brainFocus = typeof d.focus === "string" ? d.focus : null;
        brainQuery = typeof d.query === "string" ? d.query : "";
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
        };
      });
    }
    // A reviewer was made for a reported card (A2A CP-A6b): its pane opens in the Studio, which stays where it is on
    // screen — the pane is what starts the harness, so it has to exist, but the owner is not pulled out of what they do.
    const review = [
      listenBackend<{ cardId: number; projectId: number; agentId: number }>("card:review-started", (started) => {
        requestAgent({ projectId: started.projectId, agentId: started.agentId, background: true });
        ideStarted = true;
        toast(`Karte #${started.cardId}: Ein Reviewer prüft sie (Studio, Agents).`, "info");
      }),
      listenBackend<{ cardId: number; reason: string }>("card:review-blocked", (blocked) => {
        toast(`Karte #${blocked.cardId} wartet auf ein Review: ${blocked.reason}`, "warning");
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
        const note = runEventNote(event);
        if (note) toast(note.text, note.tone);
      }),
      // A card session used up a limit and was stopped (A2A CP-A6c): the card keeps it, the owner decides.
      listenBackend<{ cardId: number | null; planId: number | null; agentName: string; reason: string }>(
        "card:limit-reached",
        (stopped) => {
          const what = stopped.cardId !== null ? `Karte #${stopped.cardId}` : `Plan #${stopped.planId}`;
          toast(`${what}: ${stopped.agentName} ist am Limit gestoppt (${stopped.reason}).`, "warning");
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
    return () => {
      offs.forEach((off) => off());
      review.forEach((pending) => void pending.then((off) => off()));
    };
  });
</script>

<TopBar />
<IconBar />
<Toasts />
<ModulePicker bind:open={pickerOpen} />
<Settings bind:open={settingsOpen} />
{#if brainOpen}
  <SecondBrainView bind:open={brainOpen} focus={brainFocus} initialQuery={brainQuery} />
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

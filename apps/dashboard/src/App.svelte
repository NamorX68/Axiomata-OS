<!-- Shell composition: TopBar → IconBar → Canvas → AssistantBar, plus overlays. -->
<script lang="ts">
  import { onMount } from "svelte";

  import Canvas from "./canvas/Canvas.svelte";
  import { emit, on } from "./core/bus";
  import { openStaged } from "./core/staging";
  import { loadInstances } from "./core/stores";
  import AssistantBar from "./shell/AssistantBar.svelte";
  import ChatPanel from "./shell/ChatPanel.svelte";
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
        openStaged("md-file", { path: d.path, mode: d.mode === "edit" ? "edit" : "read" });
      }),
      on("shell:settings", () => (settingsOpen = true)),
      on("shell:ide", () => {
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
      on("shell:new-note", () => openStaged("md-file", { path: "", mode: "edit", isNew: true })),
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
        };
      });
    }
    return () => offs.forEach((off) => off());
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
     line in the IDE. Hidden rather than unmounted so a half-typed prompt is
     still there when the IDE is closed again. -->
<div class="assistant-host" class:hidden={ideOpen}>
  <AssistantBar />
</div>

<style>
  .assistant-host.hidden {
    display: none;
  }
</style>

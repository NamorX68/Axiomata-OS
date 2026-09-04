<!-- Shell composition: TopBar → IconBar → Canvas → AssistantBar, plus overlays. -->
<script lang="ts">
  import { onMount } from "svelte";

  import Canvas from "./canvas/Canvas.svelte";
  import { emit, on } from "./core/bus";
  import { openStaged } from "./core/staging";
  import { loadInstances } from "./core/stores";
  import AssistantBar from "./shell/AssistantBar.svelte";
  import ChatPanel from "./shell/ChatPanel.svelte";
  import IconBar from "./shell/IconBar.svelte";
  import ModulePicker from "./shell/ModulePicker.svelte";
  import NewNotePanel from "./shell/NewNotePanel.svelte";
  import SecondBrainView from "./shell/SecondBrainView.svelte";
  import Settings from "./shell/Settings.svelte";
  import StagingLayer from "./shell/StagingLayer.svelte";
  import Toasts from "./shell/Toasts.svelte";
  import TopBar from "./shell/TopBar.svelte";

  let pickerOpen = $state(false);
  let settingsOpen = $state(false);
  let newNoteOpen = $state(false);
  let brainOpen = $state(false);
  let brainFocus = $state<string | null>(null);
  let brainQuery = $state("");

  onMount(() => {
    const offs = [
      on("shell:add-module", () => (pickerOpen = true)),
      // A file handed over by a module, the chat or the agent → staged viewer.
      on("open-file", (detail) => {
        const d = (detail ?? {}) as { path?: string; from?: "bottom" | "right"; mode?: string };
        if (typeof d.path !== "string") return;
        openStaged("md-file", { path: d.path, mode: d.mode === "edit" ? "edit" : "read" }, d.from ?? "right");
      }),
      on("shell:settings", () => (settingsOpen = true)),
      on("shell:new-note", () => (newNoteOpen = true)),
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
<NewNotePanel bind:open={newNoteOpen} />
{#if brainOpen}
  <SecondBrainView bind:open={brainOpen} focus={brainFocus} initialQuery={brainQuery} />
{/if}
<StagingLayer />
<ChatPanel />

<Canvas />
<AssistantBar />

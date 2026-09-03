<!-- Shell composition: TopBar → IconBar → Canvas → AssistantBar, plus overlays. -->
<script lang="ts">
  import { onMount } from "svelte";

  import Canvas from "./canvas/Canvas.svelte";
  import { emit, on } from "./core/bus";
  import { openStaged } from "./core/staging";
  import AssistantBar from "./shell/AssistantBar.svelte";
  import ChatPanel from "./shell/ChatPanel.svelte";
  import IconBar from "./shell/IconBar.svelte";
  import ModulePicker from "./shell/ModulePicker.svelte";
  import StagingLayer from "./shell/StagingLayer.svelte";
  import Toasts from "./shell/Toasts.svelte";
  import TopBar from "./shell/TopBar.svelte";

  let pickerOpen = $state(false);

  // Settings (step 13) is still a stub; "search" focuses the assistant bar.
  onMount(() => {
    const offs = [
      on("shell:add-module", () => (pickerOpen = true)),
      // A file handed over by a module, the chat or the agent → staged viewer.
      on("open-file", (detail) => {
        const d = (detail ?? {}) as { path?: string; from?: "bottom" | "right"; mode?: string };
        if (typeof d.path !== "string") return;
        openStaged("md-file", { path: d.path, mode: d.mode === "edit" ? "edit" : "read" }, d.from ?? "right");
      }),
      on("shell:settings", () => console.info("shell:settings — not wired yet")),
    ];
    if (import.meta.env.DEV) {
      // Browser-console handle for driving the shell without Tauri.
      (window as unknown as { __ax: unknown }).__ax = { emit, openStaged };
    }
    return () => offs.forEach((off) => off());
  });
</script>

<TopBar />
<IconBar />
<Toasts />
<ModulePicker bind:open={pickerOpen} />
<StagingLayer />
<ChatPanel />

<Canvas />
<AssistantBar />

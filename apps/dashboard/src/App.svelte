<!-- Shell composition: TopBar → IconBar → Canvas → (assistant bar, step 11). -->
<script lang="ts">
  import { onMount } from "svelte";

  import Canvas from "./canvas/Canvas.svelte";
  import { emit, on } from "./core/bus";
  import { openStaged } from "./core/staging";
  import IconBar from "./shell/IconBar.svelte";
  import ModulePicker from "./shell/ModulePicker.svelte";
  import StagingLayer from "./shell/StagingLayer.svelte";
  import Toasts from "./shell/Toasts.svelte";
  import TopBar from "./shell/TopBar.svelte";

  let pickerOpen = $state(false);

  // Stub handlers for the icon-bar events until their dialogs exist
  // (settings: step 13, search: step 11).
  onMount(() => {
    const offs = [
      on("shell:add-module", () => (pickerOpen = true)),
      // A file handed over by a module, the chat or the agent → staged viewer.
      on("open-file", (detail) => {
        const d = (detail ?? {}) as { path?: string; from?: "bottom" | "right"; mode?: string };
        if (typeof d.path !== "string") return;
        openStaged("md-file", { path: d.path, mode: d.mode === "edit" ? "edit" : "read" }, d.from ?? "right");
      }),
      ...["shell:settings", "shell:search"].map((ev) =>
        on(ev, () => console.info(`${ev} — not wired yet`)),
      ),
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

<Canvas />

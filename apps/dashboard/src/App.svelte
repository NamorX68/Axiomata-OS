<!-- Shell composition: TopBar → IconBar → Canvas → (assistant bar, step 11). -->
<script lang="ts">
  import { onMount } from "svelte";

  import Canvas from "./canvas/Canvas.svelte";
  import { on } from "./core/bus";
  import IconBar from "./shell/IconBar.svelte";
  import ModulePicker from "./shell/ModulePicker.svelte";
  import Toasts from "./shell/Toasts.svelte";
  import TopBar from "./shell/TopBar.svelte";

  let pickerOpen = $state(false);

  // Stub handlers for the icon-bar events until their dialogs exist
  // (settings: step 13, search: step 11).
  onMount(() => {
    const offs = [
      on("shell:add-module", () => (pickerOpen = true)),
      ...["shell:settings", "shell:search"].map((ev) =>
        on(ev, () => console.info(`${ev} — not wired yet`)),
      ),
    ];
    return () => offs.forEach((off) => off());
  });
</script>

<TopBar />
<IconBar />
<Toasts />
<ModulePicker bind:open={pickerOpen} />

<Canvas />

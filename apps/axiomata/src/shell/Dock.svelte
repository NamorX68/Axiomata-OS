<!--
  The dock (`docs/plans/ui-ux-round.md` UX3, G5): one floating bar at the top centre in place of the old title block
  and its six unlabelled icons. The brand on the left (hexagon, name, the workspace), then the places to go — Studio,
  Kanban, Second Brain, a new note — with their names, today's heartbeat and spend (`Heartbeat.svelte`), the spotlight
  as a search field with its key, and the quiet tools at the end (add a module, the theme choice, the settings).

  The Studio's hex mark glows while card sessions are at work (`core/studioActivity.ts`, the same signal as the Orbit's
  live pulse). Below ~1500 px of window width the names go and the icons stay, with the name and key as tooltip.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { type AppInfo, invokeBackend as invoke } from "../core/backend";
  import { emit } from "../core/bus";
  import { keyHints } from "../core/keyHints";
  import { platform } from "../core/keys";
  import { studioActivity } from "../core/studioActivity";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import type { IconName } from "../ui/icons/lucide";
  import StatusMark from "../ui/StatusMark.svelte";
  import Heartbeat from "./Heartbeat.svelte";
  import ThemePicker from "./ThemePicker.svelte";

  interface Target {
    id: string;
    label: string;
    icon: IconName;
    go: () => void;
  }

  const TARGETS: readonly Target[] = [
    { id: "studio", label: "Studio", icon: "code", go: () => emit("shell:studio") },
    { id: "kanban", label: "Kanban", icon: "square-kanban", go: () => emit("shell:kanban") },
    { id: "brain", label: "Brain", icon: "brain", go: () => emit("open-second-brain", {}) },
    { id: "note", label: "Note", icon: "square-pen", go: () => emit("shell:new-note") },
  ];

  let info = $state<AppInfo | null>(null);
  let themeOpen = $state(false);

  const spotlightKey = $derived(keyHints("⌘K", $platform));
  const settingsKey = $derived(keyHints("⌘,", $platform));
  const working = $derived($studioActivity > 0);

  onMount(async () => {
    try {
      info = await invoke<AppInfo>("get_app_info");
    } catch (err) {
      console.error("get_app_info failed:", err);
    }
  });
</script>

<header class="dock-row">
  <nav class="dock" aria-label="Dashboard actions">
    <div class="brand" title={info ? `${info.owner} · ${info.workspace_root} · v${info.version}` : ""}>
      <svg class="logo" viewBox="0 0 24 24" aria-hidden="true">
        <path d="M12 2.5l8.2 4.75v9.5L12 21.5l-8.2-4.75v-9.5z" fill="currentColor" />
        <path d="M12 7.5l4.3 2.5v5L12 17.5l-4.3-2.5v-5z" class="core" />
      </svg>
      <h1 class="name">Axiomata</h1>
      {#if info?.workspace_name}<span class="workspace">{info.workspace_name}</span>{/if}
    </div>

    <span class="sep" aria-hidden="true"></span>

    {#each TARGETS as t (t.id)}
      <button type="button" class="target" title={t.label} aria-label={t.label} onclick={t.go}>
        <Icon name={t.icon} />
        <span class="label">{t.label}</span>
        {#if t.id === "studio" && working}
          <span class="live"><StatusMark tone="live" label="{$studioActivity} at work" /></span>
        {/if}
      </button>
    {/each}

    <span class="sep" aria-hidden="true"></span>
    <Heartbeat />
    <span class="sep" aria-hidden="true"></span>

    <button
      type="button"
      class="search"
      title="Search or run ({spotlightKey})"
      aria-label="Search or run"
      onclick={() => emit("shell:spotlight")}
    >
      <Icon name="search" />
      <span class="label">Search or run…</span>
      <kbd>{spotlightKey}</kbd>
    </button>

    <IconButton icon="layout-grid" label="Add module" onclick={() => emit("shell:add-module")} />
    <span class="theme-slot" data-theme-toggle>
      <IconButton icon="palette" label="Theme" pressed={themeOpen} onclick={() => (themeOpen = !themeOpen)} />
      {#if themeOpen}<ThemePicker onClose={() => (themeOpen = false)} />{/if}
    </span>
    <IconButton icon="settings" label="Settings ({settingsKey})" onclick={() => emit("shell:settings")} />
  </nav>
</header>

<style>
  /* Above the canvas: the dock's glass makes it a stacking context of its own, which would trap its popovers (theme,
     run) under the tiles. */
  .dock-row {
    position: relative;
    z-index: var(--ax-z-dock);
    display: flex;
    justify-content: center;
    flex: 0 0 auto;
    padding: var(--ax-space-3) var(--ax-space-4) var(--ax-space-2);
  }

  .dock {
    display: flex;
    align-items: center;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-lg);
    background: color-mix(in srgb, var(--ax-elev-2) 82%, transparent);
    -webkit-backdrop-filter: blur(var(--ax-tile-glass-blur));
    backdrop-filter: blur(var(--ax-tile-glass-blur));
    box-shadow: var(--ax-shadow-panel);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: 0 var(--ax-space-2) 0 var(--ax-space-1);
    white-space: nowrap;
  }

  /* The logo is the one hexagon that always glows a little: the system's mark. */
  .logo {
    width: calc(20px * var(--ax-ui-scale));
    height: calc(22px * var(--ax-ui-scale));
    color: var(--ax-glow);
    flex: none;
  }

  .logo .core {
    fill: var(--ax-elev-2);
  }

  .name {
    margin: 0;
    font: var(--ax-type-title);
    letter-spacing: var(--ax-tracking-tight);
  }

  .workspace {
    font: var(--ax-type-caption);
    color: var(--ax-text-muted);
  }

  .sep {
    width: 1px;
    height: calc(22px * var(--ax-ui-scale));
    margin: 0 var(--ax-space-1);
    background: var(--ax-border-strong);
  }

  .target,
  .search {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-2);
    min-height: var(--ax-hit-min);
    padding: var(--ax-space-1) var(--ax-space-3);
    border: 0;
    border-radius: var(--ax-radius-md);
    background: transparent;
    color: var(--ax-text-muted);
    font: var(--ax-type-body);
    font-weight: 500;
    cursor: pointer;
    transition:
      background var(--ax-dur-fast) var(--ax-ease),
      color var(--ax-dur-fast) var(--ax-ease);
  }

  .target:hover,
  .search:hover {
    background: var(--ax-selected-bg);
    color: var(--ax-text);
  }

  .target:focus-visible,
  .search:focus-visible {
    outline: 2px solid var(--ax-focus-ring);
    outline-offset: 1px;
  }

  .live {
    position: absolute;
    top: calc(3px * var(--ax-ui-scale));
    right: calc(4px * var(--ax-ui-scale));
    display: inline-flex;
  }

  .search {
    min-width: calc(220px * var(--ax-ui-scale));
    margin-right: var(--ax-space-1);
    border: 1px solid var(--ax-border);
    background: var(--ax-elev-1);
  }

  .search kbd {
    margin-left: auto;
    padding: 0 var(--ax-space-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-sm);
    font: var(--ax-type-caption);
    line-height: 1.5;
    color: var(--ax-text-muted);
  }

  .theme-slot {
    position: relative;
    display: inline-flex;
  }

  /* A narrow window keeps the icons, the names go into the tooltips (G5). */
  @media (max-width: 1500px) {
    .label,
    .workspace {
      display: none;
    }

    .target {
      padding: var(--ax-space-1) var(--ax-space-2);
    }

    .search {
      min-width: 0;
    }
  }
</style>

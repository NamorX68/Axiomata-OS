<!--
  Top-centre identity block: hexagon logo, "Axiomata <em>Agentic OS</em>" and
  the owner line ("<owner> | <workspace>") from `get_app_info`. Absolutely
  centred so it stays at the true centre regardless of what the icon bar or
  future side panels do.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  interface AppInfo {
    owner: string;
    workspace_name: string;
    workspace_root: string;
    version: string;
  }

  let info = $state<AppInfo | null>(null);

  const ownerLine = $derived.by(() => {
    if (!info) return "";
    return [info.owner, info.workspace_name].filter((s) => s.length > 0).join("  |  ");
  });

  onMount(async () => {
    try {
      info = await invoke<AppInfo>("get_app_info");
    } catch (err) {
      console.error("get_app_info failed:", err);
    }
  });
</script>

<header class="topbar">
  <div class="identity" title={info ? `${info.workspace_root} · v${info.version}` : ""}>
    <h1>
      <svg class="logo" viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M12 2.5l8.2 4.75v9.5L12 21.5l-8.2-4.75v-9.5z"
          fill="none"
          stroke="currentColor"
          stroke-width="1.8"
          stroke-linejoin="round"
        />
        <path
          d="M12 7.5l4.3 2.5v5L12 17.5l-4.3-2.5v-5z"
          fill="currentColor"
          opacity="0.35"
        />
      </svg>
      <span class="name">Axiomata</span>
      <em>Agentic OS</em>
    </h1>
    <p class="owner">{ownerLine || " "}</p>
  </div>
</header>

<style>
  .topbar {
    position: relative;
    height: 84px;
    flex: 0 0 auto;
  }

  .identity {
    position: absolute;
    left: 50%;
    top: var(--ax-space-4);
    transform: translateX(-50%);
    text-align: center;
    white-space: nowrap;
  }

  h1 {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--ax-space-2);
    font-size: 28px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .logo {
    width: 26px;
    height: 26px;
    color: var(--ax-accent);
    flex: 0 0 auto;
  }

  .name {
    color: var(--ax-text);
  }

  em {
    font-style: normal;
    font-weight: 500;
    color: var(--ax-accent);
  }

  .owner {
    margin: 2px 0 0;
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text-muted);
  }
</style>

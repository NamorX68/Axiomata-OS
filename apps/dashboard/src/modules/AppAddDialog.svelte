<!--
  "+" dialog for the App Ring's right (external Mac apps) side: lists apps
  found by `list_installed_apps` with a search field. Each row's button
  toggles between "Hinzufügen"/"Entfernen" depending on whether the app's
  path is already in `userApps` — active in both states (unlike
  `ModulePicker`'s `disabled` for a placed singleton, the second state here
  is a real, reversible action), instant, no confirmation: a deliberate
  click on one row of a searchable list is structurally far less likely to
  be accidental than the ring's own right-click, which does get a confirm
  step (see AppContextMenu.svelte). Chrome is the shared `Window.svelte`.
-->
<script lang="ts">
  import { addUserApp, removeUserApp, userApps } from "../core/apps";
  import { invokeBackend } from "../core/backend";
  import type { InstalledApp } from "../core/backend";
  import Window from "../shell/Window.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let apps = $state<InstalledApp[]>([]);
  let truncated = $state(false);
  let loading = $state(false);
  let error = $state("");
  let query = $state("");

  const addedPaths = $derived(new Set($userApps.map((a) => a.path)));
  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? apps.filter((a) => a.name.toLowerCase().includes(q)) : apps;
  });

  // Re-scans every time the dialog opens rather than caching — a `.app`
  // list changes rarely and a fresh scan is cheap, so there's no staleness
  // trade-off worth a cache invalidation story.
  async function load() {
    loading = true;
    error = "";
    try {
      const result = await invokeBackend<{ apps: InstalledApp[]; truncated: boolean }>("list_installed_apps");
      apps = result.apps;
      truncated = result.truncated;
    } catch (err) {
      error = String(err);
    } finally {
      loading = false;
    }
  }

  function toggle(app: InstalledApp) {
    if (addedPaths.has(app.path)) removeUserApp(app.path);
    else addUserApp(app);
  }

  $effect(() => {
    if (open) void load();
  });
</script>

{#if open}
  <Window
    title="Add app"
    onClose={() => (open = false)}
    style="width: min(420px, calc(100vw - 2 * var(--ax-space-5))); height: min(560px, calc(100vh - 2 * var(--ax-space-5)));"
  >
    <div class="content">
      <input type="search" class="filter" placeholder="Search…" aria-label="Filter apps" bind:value={query} />
      {#if loading}
        <p class="muted">Loading…</p>
      {:else if error}
        <p class="muted">{error}</p>
      {:else if filtered.length === 0}
        <p class="muted">Keine Apps gefunden.</p>
      {:else}
        <ul>
          {#each filtered as app (app.path)}
            <li>
              <span class="name">{app.name}</span>
              <button type="button" class="toggle" onclick={() => toggle(app)}>
                {addedPaths.has(app.path) ? "Entfernen" : "Hinzufügen"}
              </button>
            </li>
          {/each}
        </ul>
        {#if truncated}
          <p class="muted hint">Weitere Apps nicht angezeigt — Suchfeld nutzen.</p>
        {/if}
      {/if}
    </div>
  </Window>
{/if}

<style>
  .content {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 0 var(--ax-space-3) var(--ax-space-3);
  }

  .filter {
    width: 100%;
    flex: 0 0 auto;
    margin-bottom: var(--ax-space-2);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow: auto;
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }

  li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2) var(--ax-space-3);
    background: var(--ax-surface-2);
    border-radius: var(--ax-radius-md);
  }
  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .toggle {
    flex: 0 0 auto;
  }

  .muted {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
  .hint {
    margin-top: var(--ax-space-2);
    flex: 0 0 auto;
  }
</style>

<!--
  "+" dialog for the App Ring's right (external Mac apps) side: lists apps
  found by `list_installed_apps` with a search field. Each row's button
  toggles between "Hinzufügen"/"Entfernen" depending on whether the app's
  path is already in `userApps` — active in both states (unlike
  `ModulePicker`'s `disabled` for a placed singleton, the second state here
  is a real, reversible action), instant, no confirmation: a deliberate
  click on one row of a searchable list is structurally far less likely to
  be accidental than the ring's own right-click, which does get a confirm
  step (see AppContextMenu.svelte). Styling mirrors `ModulePicker.svelte`.
-->
<script lang="ts">
  import { addUserApp, removeUserApp, userApps } from "../core/apps";
  import { invokeBackend } from "../core/backend";
  import type { InstalledApp } from "../core/backend";

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

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") open = false;
  }

  $effect(() => {
    if (open) void load();
  });
</script>

<svelte:window onkeydown={open ? onKeydown : undefined} />

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="overlay" onclick={() => (open = false)}>
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="app-add-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
    >
      <header>
        <h2 id="app-add-title">Add app</h2>
        <button type="button" class="close" aria-label="Close" onclick={() => (open = false)}>
          ×
        </button>
      </header>
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
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: var(--ax-z-dialog);
    display: grid;
    place-items: center;
    background: var(--ax-overlay);
  }

  .dialog {
    width: min(420px, calc(100vw - 2 * var(--ax-space-5)));
    max-height: calc(100vh - 2 * var(--ax-space-5));
    display: flex;
    flex-direction: column;
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-pop);
    padding: var(--ax-space-2) var(--ax-space-4) var(--ax-space-4);
  }

  header {
    display: flex;
    align-items: center;
    padding: var(--ax-space-3) 0;
    border-bottom: 1px solid var(--ax-border);
    margin-bottom: var(--ax-space-3);
  }
  h2 {
    flex: 1 1 auto;
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }
  .close {
    width: 24px;
    height: 24px;
    padding: 0;
    line-height: 1;
    background: transparent;
    border-color: transparent;
    color: var(--ax-text-muted);
  }

  .filter {
    width: 100%;
    margin-bottom: var(--ax-space-2);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow: auto;
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
  }
</style>

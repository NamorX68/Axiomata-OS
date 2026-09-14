<!--
  "+" dialog for the App Ring: an "Intern"/"Extern" mode switch (Checkpoint
  5c of docs/plans/app-ring.md) over what was originally the external-only
  dialog.

  "Extern" (the original, only mode before 5c): lists Mac apps found by
  `list_installed_apps` with a search field. Each row's button toggles
  between "Hinzufügen"/"Entfernen" depending on whether the app's path is
  already in `userApps`.

  "Intern": lists every ring-eligible builtin Tool (a singleton module —
  Memory, Skills, Routines, ToDo, Calendar, Reminders, Mail — brings an
  existing instance to front on a ring click) and App (`singleton: false` —
  currently only Terminal — always spawns a fresh instance on a ring click;
  see `second-brain.svelte`'s `handleAppClick`) via
  `listAllRingEligibleBuiltins()`, one combined list rather than two
  separately-headed ones (owner decision) — the Tool/App distinction only
  shows up in the ring's own click behaviour, not in this list. Each row's
  button toggles between "Anzeigen"/"Ausblenden" depending on whether the
  type is in `hiddenBuiltins` — deliberately different verbs from the
  external side's "Hinzufügen"/"Entfernen": a builtin is never actually
  removed from the app, only hidden from the ring, and can always be found
  again here (unlike a Mac app, which really does leave `userApps`
  entirely). Before 5c, builtins had no user-facing ring management at all —
  every registered one just always showed, and only Claude editing
  `isRingEligible`'s allow-list could change that.

  Both modes' toggle button is active in both states (unlike `ModulePicker`'s
  `disabled` for a placed singleton, the second state in both lists here is
  a real, reversible action), instant, no confirmation: a deliberate click
  on one row of a searchable list is structurally far less likely to be
  accidental than the ring's own right-click, which does get a confirm step
  for the actions that have one (see AppContextMenu.svelte — builtins still
  get no right-click "Entfernen" there at all; hiding one only happens here).
  Switching modes clears the search field — a query typed for one list
  landing on the other's differently-shaped results would be confusing, not
  helpful. Chrome is the shared `Window.svelte`.
-->
<script lang="ts">
  import {
    addUserApp,
    hiddenBuiltins,
    hideBuiltinApp,
    listAllRingEligibleBuiltins,
    removeUserApp,
    showBuiltinApp,
    userApps,
    type BuiltinApp,
  } from "../core/apps";
  import { invokeBackend } from "../core/backend";
  import type { InstalledApp } from "../core/backend";
  import Window from "../shell/Window.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  type Mode = "extern" | "intern";
  let mode = $state<Mode>("extern");

  let apps = $state<InstalledApp[]>([]);
  let truncated = $state(false);
  let loading = $state(false);
  let error = $state("");
  let query = $state("");

  const addedPaths = $derived(new Set($userApps.map((a) => a.path)));
  const filteredExtern = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? apps.filter((a) => a.name.toLowerCase().includes(q)) : apps;
  });

  // Static after `registerBuiltins()` at boot — unlike the external list,
  // there's nothing to (re-)fetch, so this reads straight off the registry
  // on every render rather than needing its own loaded/loading state.
  const builtins = listAllRingEligibleBuiltins();
  const hiddenTypes = $derived(new Set($hiddenBuiltins));
  const filteredIntern = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return q ? builtins.filter((a) => a.title.toLowerCase().includes(q)) : builtins;
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

  function toggleExtern(app: InstalledApp) {
    if (addedPaths.has(app.path)) removeUserApp(app.path);
    else addUserApp(app);
  }

  function toggleIntern(app: BuiltinApp) {
    if (hiddenTypes.has(app.type)) showBuiltinApp(app.type);
    else hideBuiltinApp(app.type);
  }

  function setMode(next: Mode) {
    mode = next;
    query = "";
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
      <div class="mode-switch" role="radiogroup" aria-label="Intern oder extern">
        <label class:active={mode === "intern"}>
          <input type="radio" name="app-add-dialog-mode" checked={mode === "intern"} onchange={() => setMode("intern")} />
          Intern
        </label>
        <label class:active={mode === "extern"}>
          <input type="radio" name="app-add-dialog-mode" checked={mode === "extern"} onchange={() => setMode("extern")} />
          Extern
        </label>
      </div>
      <input type="search" class="filter" placeholder="Search…" aria-label="Filter apps" bind:value={query} />
      {#if mode === "intern"}
        {#if filteredIntern.length === 0}
          <p class="muted">Keine Module gefunden.</p>
        {:else}
          <ul>
            {#each filteredIntern as app (app.type)}
              <li>
                <span class="name">{app.title}</span>
                <button type="button" class="toggle" onclick={() => toggleIntern(app)}>
                  {hiddenTypes.has(app.type) ? "Anzeigen" : "Ausblenden"}
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      {:else if loading}
        <p class="muted">Loading…</p>
      {:else if error}
        <p class="muted">{error}</p>
      {:else if filteredExtern.length === 0}
        <p class="muted">Keine Apps gefunden.</p>
      {:else}
        <ul>
          {#each filteredExtern as app (app.path)}
            <li>
              <span class="name">{app.name}</span>
              <button type="button" class="toggle" onclick={() => toggleExtern(app)}>
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

  .mode-switch {
    display: flex;
    flex: 0 0 auto;
    gap: var(--ax-space-1);
    margin-bottom: var(--ax-space-2);
    padding: 2px;
    background: var(--ax-surface-2);
    border-radius: var(--ax-radius-md);
  }
  /* Real `<input type="radio">` for the actual semantics/keyboard nav
   *  (matching `calendar-settings.svelte`'s existing `role="radiogroup"`
   *  convention), visually styled as a segmented control by hiding the
   *  native appearance and treating the `<label>` as the pill instead —
   *  architecture review, Checkpoint 5c: the original plain-`<button>`
   *  version announced as a radio group without any actual radio
   *  semantics for a screen reader. */
  .mode-switch label {
    flex: 1 1 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--ax-space-1) var(--ax-space-2);
    border-radius: var(--ax-radius-sm);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }
  .mode-switch label.active {
    background: var(--ax-surface-1);
    color: var(--ax-text);
  }
  .mode-switch input[type="radio"] {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  /* The native radio itself is invisible (styled via its `<label>` above),
   *  but keyboard-tabbing to it must still show a visible focus cue — a
   *  sighted keyboard user shouldn't lose track of which pill is focused
   *  just because the control rendering it is hidden. */
  .mode-switch label:has(input:focus-visible) {
    outline: 2px solid var(--ax-accent);
    outline-offset: -2px;
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

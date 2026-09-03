<!--
  Settings dialog: built-in theme picker (applies instantly, persists via
  the theme store), custom-CSS status with reload / copy-template, and the
  read-only app facts (owner, workspace, version).
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { type AppInfo, invokeBackend } from "../core/backend";
  import { customTheme, loadCustomTheme } from "../core/custom-theme";
  import { activeTheme } from "../core/stores";
  import { THEMES, applyTheme } from "../core/themes";
  import { toast } from "../core/toast";
  import { TEMPLATE } from "../theme/validator";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let info = $state<AppInfo | null>(null);
  let reloading = $state(false);
  let showTemplate = $state(false);

  async function reload() {
    reloading = true;
    const state = await loadCustomTheme();
    reloading = false;
    toast(state.message, state.status === "applied" || state.status === "absent" ? "info" : "warning");
  }

  async function copyTemplate() {
    try {
      await navigator.clipboard.writeText(TEMPLATE);
      toast("Template copied — save it as ~/.axiomata/theme.css.");
    } catch {
      showTemplate = true;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") open = false;
  }

  onMount(async () => {
    try {
      info = await invokeBackend<AppInfo>("get_app_info");
    } catch {
      info = null;
    }
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
      aria-labelledby="settings-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
    >
      <header>
        <h2 id="settings-title">Settings</h2>
        <button type="button" class="close" aria-label="Close" onclick={() => (open = false)}>×</button>
      </header>

      <div class="body">
        <section>
          <h3>Theme</h3>
          <ul class="themes">
            {#each THEMES as t (t.id)}
              <li>
                <button
                  type="button"
                  class="theme"
                  class:active={$activeTheme === t.id}
                  data-theme-id={t.id}
                  onclick={() => applyTheme(t.id)}
                >
                  <span class="swatch" data-theme={t.id}><i></i><i></i><i></i></span>
                  <span class="text">
                    <span class="title">{t.label}</span>
                    <span class="meta">{t.blurb}</span>
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        </section>

        <section>
          <h3>Custom CSS</h3>
          <p class="status {$customTheme.status}">{$customTheme.message || "Not loaded."}</p>
          {#if $customTheme.errors.length > 0}
            <ul class="errors">
              {#each $customTheme.errors as e, i (i)}
                <li><code>{e.rule}</code>{#if e.property} · <code>{e.property}</code>{/if} — {e.message}</li>
              {/each}
            </ul>
          {/if}
          <div class="actions">
            <button type="button" disabled={reloading} onclick={reload}>{reloading ? "Reloading…" : "Reload custom CSS"}</button>
            <button type="button" onclick={copyTemplate}>Copy template</button>
          </div>
          {#if showTemplate}
            <textarea readonly rows="8" spellcheck="false">{TEMPLATE}</textarea>
          {/if}
        </section>

        <section>
          <h3>About</h3>
          {#if info}
            <dl>
              <dt>Owner</dt><dd>{info.owner || "— (set `owner` in ~/.axiomata/config.toml)"}</dd>
              <dt>Workspace</dt><dd>{info.workspace_root}</dd>
              <dt>Version</dt><dd>{info.version}</dd>
            </dl>
          {/if}
        </section>
      </div>
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
    width: min(620px, calc(100vw - 2 * var(--ax-space-5)));
    max-height: calc(100vh - 2 * var(--ax-space-5));
    display: flex;
    flex-direction: column;
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-pop);
  }
  header {
    display: flex;
    align-items: center;
    padding: var(--ax-space-3) var(--ax-space-4);
    border-bottom: 1px solid var(--ax-border);
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

  .body {
    overflow: auto;
    padding: var(--ax-space-3) var(--ax-space-4) var(--ax-space-4);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-4);
  }
  h3 {
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-text-muted);
    margin-bottom: var(--ax-space-2);
  }

  .themes {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: var(--ax-space-2);
  }
  .theme {
    width: 100%;
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2);
    text-align: left;
    background: var(--ax-surface-2);
  }
  .theme.active {
    border-color: var(--ax-accent);
    background: var(--ax-accent-muted);
  }
  .swatch {
    display: inline-flex;
    gap: 2px;
    flex: 0 0 auto;
  }
  .swatch i {
    width: 10px;
    height: 22px;
    border-radius: 2px;
  }
  /* Swatches use the theme files' own tokens by scoping the data-theme. */
  .swatch i:nth-child(1) {
    background: var(--ax-bg);
    border: 1px solid var(--ax-border-strong);
  }
  .swatch i:nth-child(2) {
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border-strong);
  }
  .swatch i:nth-child(3) {
    background: var(--ax-accent);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .title {
    font-weight: 600;
  }
  .meta {
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .status {
    margin: 0 0 var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .status.applied {
    color: var(--ax-success);
  }
  .status.invalid,
  .status.error {
    color: var(--ax-warning);
  }
  .errors {
    margin: 0 0 var(--ax-space-2);
    padding-left: var(--ax-space-4);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-danger);
  }
  .errors code {
    font-family: var(--ax-font-mono);
    color: var(--ax-text);
  }
  .actions {
    display: flex;
    gap: var(--ax-space-2);
  }
  textarea {
    width: 100%;
    margin-top: var(--ax-space-2);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    resize: vertical;
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--ax-space-1) var(--ax-space-4);
    margin: 0;
    font-size: var(--ax-font-size-sm);
  }
  dt {
    color: var(--ax-text-muted);
  }
  dd {
    margin: 0;
    word-break: break-all;
  }
</style>

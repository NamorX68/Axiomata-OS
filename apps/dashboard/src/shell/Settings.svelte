<!--
  Settings dialog: built-in theme picker (applies instantly, persists via
  the theme store), custom-CSS status with reload / copy-template, the
  editable Vault path + model-provider config (both persisted through
  `save_config`), and the read-only app facts (version).
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { type AppInfo, type Config, type ProviderId, type SpendSummary, invokeBackend } from "../core/backend";
  import { customTheme, loadCustomTheme } from "../core/custom-theme";
  import { activeTheme, showGrid, snapEdges } from "../core/stores";
  import { THEMES, applyTheme } from "../core/themes";
  import { toast } from "../core/toast";
  import { TEMPLATE } from "../theme/validator";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let info = $state<AppInfo | null>(null);
  let reloading = $state(false);
  let showTemplate = $state(false);

  /** The full editable config from `get_config`. `base_url` / `api_key`
   *  nulls are normalised to `""` on load so `<input>` can bind to them;
   *  `cleanConfig` reverses that before every `save_config`. */
  let config = $state<Config | null>(null);
  let savingVault = $state(false);
  let savingProvider = $state(false);

  /** Recorded agent spend for the active provider (`get_spend_summary`),
   *  refreshed on open and after every provider save. */
  let spend = $state<SpendSummary | null>(null);
  async function refreshSpend() {
    try {
      spend = await invokeBackend<SpendSummary>("get_spend_summary");
    } catch {
      spend = null;
    }
  }
  /** Empty field = no cap (`null`); any number is the daily USD cap. */
  function setDailyCap(raw: string) {
    if (!config) return;
    config.agents.daily_usd_cap = raw.trim() === "" ? null : Number(raw);
  }

  /** Provider picker metadata, ordered to match Rust's `ProviderId::ALL`.
   *  `key`: none = billed via the CLI's own subscription login (Anthropic);
   *  required = OpenRouter; optional = local Ollama (a placeholder token,
   *  never a real credential). */
  const PROVIDERS: { id: ProviderId; label: string; blurb: string; key: "none" | "required" | "optional" }[] = [
    { id: "anthropic", label: "Anthropic", blurb: "Über das Abo abgerechnet — kein Schlüssel", key: "none" },
    { id: "open_router", label: "OpenRouter", blurb: "Anthropic-kompatibler Endpoint", key: "required" },
    { id: "ollama", label: "Ollama", blurb: "Lokales Modell auf diesem Rechner", key: "optional" },
  ];

  const activeMeta = $derived(PROVIDERS.find((p) => p.id === config?.agents.active_provider) ?? PROVIDERS[0]);
  const activeSettings = $derived(config ? config.agents.providers[config.agents.active_provider] : null);

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

  /** Reverse the load-time null→"" normalisation: an empty base URL or key
   *  is "unset", which the backend stores as `null`, not `""`. */
  function cleanConfig(c: Config): Config {
    const providers = Object.fromEntries(
      Object.entries(c.agents.providers).map(([id, p]) => [
        id,
        {
          ...p,
          base_url: p.base_url?.trim() ? p.base_url.trim() : null,
          api_key: p.api_key?.trim() ? p.api_key.trim() : null,
          chat_model: p.chat_model.trim(),
          skill_model: p.skill_model.trim(),
        },
      ]),
    ) as Config["agents"]["providers"];
    return { ...c, workspace_root: c.workspace_root.trim(), agents: { ...c.agents, providers } };
  }

  /** `save_config` returns `true` when only a restart will pick up the new
   *  workspace root (everything else applies live). */
  async function persist(): Promise<boolean> {
    if (!config) return false;
    return invokeBackend<boolean>("save_config", { newConfig: cleanConfig(config) });
  }

  async function saveVault() {
    if (!config?.workspace_root.trim()) {
      toast("Vault-Pfad darf nicht leer sein.", "warning");
      return;
    }
    savingVault = true;
    try {
      const restartNeeded = await persist();
      toast(
        restartNeeded
          ? "Vault-Pfad gespeichert — Axiomata-OS neu starten, um zu wechseln."
          : "Vault-Pfad gespeichert.",
        "info",
      );
    } catch (e) {
      toast(`Speichern fehlgeschlagen: ${e}`, "warning");
    } finally {
      savingVault = false;
    }
  }

  /** Fast-path affordance only — `Config::validate_for_save` on the Rust
   *  side is the authority. Catches the obvious mistakes (empty / bracket-
   *  unbalanced / whitespace-laden model id) before the round-trip so the
   *  toast can be specific; the 2026-09-08 incident was a `]`→`)` typo. */
  function providerLooksSane(): string | null {
    if (!config || !activeSettings) return null;
    if (activeMeta.key === "required" && !activeSettings.api_key?.trim()) {
      return `${activeMeta.label} braucht einen API-Schlüssel.`;
    }
    if (activeMeta.id === "anthropic") return null; // blank models = free CLI default
    for (const [label, value] of [
      ["Chat-Modell", activeSettings.chat_model],
      ["Skills-Modell", activeSettings.skill_model],
    ] as const) {
      const v = value.trim();
      if (!v) return `${activeMeta.label}: ${label} darf nicht leer sein.`;
      if (/\s/.test(v)) return `${activeMeta.label}: ${label} enthält Leerzeichen.`;
      const opens = (v.match(/\[/g) ?? []).length;
      const closes = (v.match(/\]/g) ?? []).length;
      if (opens !== closes || /[()]/.test(v)) {
        return `${activeMeta.label}: ${label} hat unausgeglichene Klammern — „${v}“.`;
      }
    }
    const url = activeSettings.base_url?.trim();
    if (!url || !/^https:\/\/.+/.test(url)) {
      const loopback = /^http:\/\/(localhost|127\.0\.0\.1|\[::1\])(:\d+)?(\/|$)/.test(url ?? "");
      if (!loopback) return `${activeMeta.label}: Basis-URL muss mit https:// beginnen.`;
    }
    return null;
  }

  async function saveProvider() {
    if (!config) return;
    const complaint = providerLooksSane();
    if (complaint) {
      toast(complaint, "warning");
      return;
    }
    savingProvider = true;
    try {
      await persist();
      toast("Provider-Einstellungen gespeichert — gelten ab dem nächsten Agenten-Aufruf.", "info");
      await refreshSpend();
    } catch (e) {
      toast(`Speichern fehlgeschlagen: ${e}`, "warning");
    } finally {
      savingProvider = false;
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
    try {
      const loaded = await invokeBackend<Config>("get_config");
      for (const p of Object.values(loaded.agents.providers)) {
        p.base_url ??= "";
        p.api_key ??= "";
      }
      config = loaded;
    } catch {
      config = null;
    }
    await refreshSpend();
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
          <h3>Canvas</h3>
          <label class="opt"><input type="checkbox" bind:checked={$showGrid} /> Show dot grid <span class="hint">(tiles snap to it either way)</span></label>
          <label class="opt"><input type="checkbox" bind:checked={$snapEdges} /> Magnetic edges <span class="hint">(tiles stick to their neighbours within 8 px)</span></label>
        </section>

        {#if config}
          <section>
            <h3>Vault</h3>
            <p class="lead">Wurzelordner des Second-Brain-Workspace. Ein Wechsel greift erst nach einem Neustart.</p>
            <label class="field">
              <span>Pfad</span>
              <input type="text" spellcheck="false" bind:value={config.workspace_root} placeholder="/Users/…/Vault" />
            </label>
            <div class="actions">
              <button type="button" disabled={savingVault} onclick={saveVault}>
                {savingVault ? "Speichern…" : "Speichern & Neustart"}
              </button>
            </div>
          </section>

          <section>
            <h3>Modell-Provider</h3>
            <p class="lead">
              Die Ausführung bleibt immer der Claude-Code-Agent — der Provider wählt nur den Upstream-Endpoint,
              an den <code>claude</code> zeigt.
            </p>
            <ul class="providers">
              {#each PROVIDERS as p (p.id)}
                <li>
                  <button
                    type="button"
                    class="provider"
                    class:active={config.agents.active_provider === p.id}
                    onclick={() => config && (config.agents.active_provider = p.id)}
                  >
                    <span class="text">
                      <span class="title">{p.label}</span>
                      <span class="meta">{p.blurb}</span>
                    </span>
                  </button>
                </li>
              {/each}
            </ul>

            <!--
              Bindings target the concrete `config.agents.providers[<id>]`
              path, never the `activeSettings` `$derived` — a `bind:value` to a
              property of a derived is silently dropped on the next recompute
              in Svelte 5, which quietly wiped provider fields from
              config.toml. `config` is `$state`, so the computed-member path
              below is a real two-way target and retargets when the picker
              changes `active_provider`.
            -->
            {#if config.agents.providers[config.agents.active_provider]}
              {@const pid = config.agents.active_provider}
              <div class="provider-form">
                {#if activeMeta.key === "none"}
                  <p class="hint">
                    Kein Schlüssel oder Basis-URL nötig — Anthropic wird über den CLI-Login des Abos abgerechnet.
                  </p>
                {:else}
                  <label class="field">
                    <span>Basis-URL</span>
                    <input type="text" spellcheck="false" bind:value={config.agents.providers[pid].base_url} placeholder="https://…" />
                  </label>
                  <label class="field">
                    <span>API-Schlüssel {activeMeta.key === "optional" ? "(optional)" : ""}</span>
                    <input
                      type="password"
                      autocomplete="off"
                      spellcheck="false"
                      bind:value={config.agents.providers[pid].api_key}
                      placeholder={activeMeta.key === "optional" ? "Platzhalter genügt für lokales Ollama" : "erforderlich"}
                    />
                  </label>
                {/if}
                <label class="field">
                  <span>Chat-Modell</span>
                  <input type="text" spellcheck="false" bind:value={config.agents.providers[pid].chat_model} placeholder="z. B. claude-sonnet-5" />
                </label>
                <label class="field">
                  <span>Skills-/Routinen-Modell</span>
                  <input type="text" spellcheck="false" bind:value={config.agents.providers[pid].skill_model} placeholder="z. B. claude-haiku-4-5" />
                </label>
                <p class="hint">Ein <code>model:</code> im SKILL.md-Frontmatter überschreibt das Skills-Modell weiterhin.</p>
              </div>
            {/if}

            <div class="provider-form spend-form">
              {#if config}
              {#if spend?.metered}
                <p class="hint">
                  Ausgaben <strong>{spend.provider}</strong>:
                  <strong>${spend.today_usd.toFixed(4)}</strong> heute{#if spend.daily_cap_usd != null}
                    &nbsp;/&nbsp;${spend.daily_cap_usd.toFixed(2)} Limit{/if}
                  &nbsp;·&nbsp;${spend.month_usd.toFixed(2)} diesen Monat
                </p>
              {:else if spend}
                <p class="hint">{spend.provider} wird übers Abo abgerechnet — keine Kostenerfassung.</p>
              {/if}
              <label class="field">
                <span>Tageslimit (USD, bezahlte Provider)</span>
                <input
                  type="number"
                  min="0"
                  step="0.5"
                  placeholder="leer = kein Limit"
                  value={config.agents.daily_usd_cap ?? ""}
                  oninput={(e) => setDailyCap(e.currentTarget.value)}
                />
              </label>
              <p class="hint">Erreicht die heutige Summe das Limit, wird der nächste Agenten-Aufruf über einen bezahlten Provider abgelehnt.</p>
              {/if}
            </div>

            <div class="actions">
              <button type="button" disabled={savingProvider} onclick={saveProvider}>
                {savingProvider ? "Speichern…" : "Provider speichern"}
              </button>
            </div>
          </section>
        {/if}

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
    gap: var(--ax-space-6);
  }
  h3 {
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
    color: var(--ax-accent);
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

  .opt {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    margin-bottom: var(--ax-space-1);
  }
  .hint {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  .lead {
    margin: 0 0 var(--ax-space-3);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .lead code,
  .hint code {
    font-family: var(--ax-font-mono);
    color: var(--ax-text);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    margin-bottom: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }
  .field > span {
    color: var(--ax-text-muted);
  }
  .field input {
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }
  .field input:focus {
    outline: none;
    border-color: var(--ax-accent);
  }

  .providers {
    list-style: none;
    margin: 0 0 var(--ax-space-3);
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: var(--ax-space-2);
  }
  .provider {
    width: 100%;
    padding: var(--ax-space-2);
    text-align: left;
    background: var(--ax-surface-2);
  }
  .provider.active {
    border-color: var(--ax-accent);
    background: var(--ax-accent-muted);
  }
  .provider-form {
    padding: var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    margin-bottom: var(--ax-space-3);
  }
  .provider-form .field:last-of-type {
    margin-bottom: 0;
  }
  .provider-form .hint {
    margin: var(--ax-space-2) 0 0;
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

<!--
  Settings dialog: built-in theme picker (applies instantly, persists via
  the theme store), custom-CSS status with reload / copy-template, the
  editable Vault path + model-provider config (both persisted through
  `save_config`), and the read-only app facts (version).
-->
<script lang="ts">
  import { onMount } from "svelte";

  import {
    type AppInfo,
    type ConfigUpdate,
    type ConfigView,
    type KeyUpdate,
    type ProviderId,
    type SpendSummary,
    invokeBackend,
  } from "../core/backend";
  import { customTheme, loadCustomTheme } from "../core/custom-theme";
  import { activeTheme, showGrid, snapEdges } from "../core/stores";
  import { THEMES, applyTheme } from "../core/themes";
  import { toast } from "../core/toast";
  import { TEMPLATE } from "../theme/validator";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let info = $state<AppInfo | null>(null);
  let reloading = $state(false);
  let showTemplate = $state(false);

  /** The redacted config from `get_config` — no raw API keys (CP7). `base_url`
   *  nulls are normalised to `""` on load so `<input>` can bind; `buildUpdate`
   *  reverses that. */
  let config = $state<ConfigView | null>(null);
  let savingVault = $state(false);
  let savingProvider = $state(false);

  /** Per-provider API-key edits. A provider absent from this map means its
   *  key field was not touched → the backend keeps the stored key. Present
   *  with `""` → clear it; present with a value → set it. */
  let keyEdits = $state<Partial<Record<ProviderId, string>>>({});
  function setKeyEdit(id: ProviderId, value: string) {
    keyEdits = { ...keyEdits, [id]: value };
  }

  /** Recorded agent spend, one entry per distinct provider across the chat
   *  and skill role selectors (`get_spend_summary`), refreshed on open and
   *  after every provider save. */
  let spend = $state<SpendSummary[]>([]);
  async function refreshSpend() {
    try {
      spend = await invokeBackend<SpendSummary[]>("get_spend_summary");
    } catch {
      spend = [];
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

  /** Which provider's settings the form below edits. The provider list is now
   *  an edit selector, not the role picker — the two `<select>`s above it map
   *  roles → providers. Defaults to the chat provider once the config loads. */
  let editingProvider = $state<ProviderId>("anthropic");
  const metaFor = (id: ProviderId) => PROVIDERS.find((p) => p.id === id) ?? PROVIDERS[0];
  const editingMeta = $derived(metaFor(editingProvider));

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

  /** How a provider's key should change on save, from `keyEdits`. */
  function keyUpdateFor(id: ProviderId): KeyUpdate {
    if (!(id in keyEdits)) return { kind: "keep" };
    const v = (keyEdits[id] ?? "").trim();
    return v === "" ? { kind: "clear" } : { kind: "set", value: v };
  }

  /** Build the `save_config` payload: trims fields, reverses the null→""
   *  normalisation, and turns each key field into a `KeyUpdate`. */
  function buildUpdate(c: ConfigView): ConfigUpdate {
    const providers = Object.fromEntries(
      (Object.entries(c.agents.providers) as [ProviderId, ConfigView["agents"]["providers"][ProviderId]][]).map(
        ([id, p]) => [
          id,
          {
            base_url: p.base_url?.trim() ? p.base_url.trim() : null,
            api_key: keyUpdateFor(id),
            chat_model: p.chat_model.trim(),
            skill_model: p.skill_model.trim(),
          },
        ],
      ),
    ) as ConfigUpdate["agents"]["providers"];
    return {
      owner: c.owner,
      workspace_root: c.workspace_root.trim(),
      agents: {
        ollama_model: c.agents.ollama_model,
        skill_timeout_secs: c.agents.skill_timeout_secs,
        providers,
        chat_provider: c.agents.chat_provider,
        skill_provider: c.agents.skill_provider,
        daily_usd_cap: c.agents.daily_usd_cap,
      },
    };
  }

  /** `save_config` returns `true` when only a restart will pick up the new
   *  workspace root (everything else applies live). */
  async function persist(): Promise<boolean> {
    if (!config) return false;
    return invokeBackend<boolean>("save_config", { newConfig: buildUpdate(config) });
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

  /** One role's provider: its role-relevant model field must be set + sane,
   *  its base URL https (or loopback), and its key present when required.
   *  Mirrors the per-role loop in Rust's `Config::validate_for_save`. */
  function roleProviderLooksSane(roleLabel: string, id: ProviderId, modelLabel: "chat_model" | "skill_model"): string | null {
    if (!config) return null;
    const meta = metaFor(id);
    if (id === "anthropic") return null; // blank models = free CLI default
    const s = config.agents.providers[id];
    if (!s) return `${roleLabel}: Provider „${id}“ hat keine Einstellungen.`;
    const hasKey = s.has_key || keyUpdateFor(id).kind === "set";
    if (meta.key === "required" && !hasKey) return `${roleLabel} (${meta.label}) braucht einen API-Schlüssel.`;

    const v = (modelLabel === "chat_model" ? s.chat_model : s.skill_model).trim();
    const human = modelLabel === "chat_model" ? "Chat-Modell" : "Skills-Modell";
    if (!v) return `${roleLabel} (${meta.label}): ${human} darf nicht leer sein.`;
    if (/\s/.test(v)) return `${roleLabel} (${meta.label}): ${human} enthält Leerzeichen.`;
    const opens = (v.match(/\[/g) ?? []).length;
    const closes = (v.match(/\]/g) ?? []).length;
    if (opens !== closes || /[()]/.test(v)) {
      return `${roleLabel} (${meta.label}): ${human} hat unausgeglichene Klammern — „${v}“.`;
    }
    const url = s.base_url?.trim();
    if (!url || !/^https:\/\/.+/.test(url)) {
      const loopback = /^http:\/\/(localhost|127\.0\.0\.1|\[::1\])(:\d+)?(\/|$)/.test(url ?? "");
      if (!loopback) return `${roleLabel} (${meta.label}): Basis-URL muss mit https:// beginnen.`;
    }
    return null;
  }

  /** Fast-path affordance only — `Config::validate_for_save` on the Rust side
   *  is the authority. Checks the chat provider's `chat_model` and the skill
   *  provider's `skill_model`; the 2026-09-08 incident was a `]`→`)` typo. */
  function providerLooksSane(): string | null {
    if (!config) return null;
    return (
      roleProviderLooksSane("Chat", config.agents.chat_provider, "chat_model") ??
      roleProviderLooksSane("Skills", config.agents.skill_provider, "skill_model")
    );
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
      keyEdits = {};
      await loadConfig();
      await refreshSpend();
    } catch (e) {
      toast(`Speichern fehlgeschlagen: ${e}`, "warning");
    } finally {
      savingProvider = false;
    }
  }

  async function loadConfig() {
    try {
      const loaded = await invokeBackend<ConfigView>("get_config");
      for (const p of Object.values(loaded.agents.providers)) p.base_url ??= "";
      // Keep the current edit target across a save-triggered reload; on the
      // first load (or if it vanished) fall back to the chat provider.
      if (!config || !loaded.agents.providers[editingProvider]) {
        editingProvider = loaded.agents.chat_provider;
      }
      config = loaded;
    } catch {
      config = null;
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
    await loadConfig();
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
            <h3>Provider bearbeiten</h3>
            <p class="lead">
              Die Ausführung bleibt immer der Claude-Code-Agent — der Provider wählt nur den Upstream-Endpoint,
              an den <code>claude</code> zeigt. Jeder Provider wird hier unabhängig konfiguriert;
              welcher wofür genutzt wird, steuert der nächste Abschnitt.
            </p>
            <ul class="providers">
              {#each PROVIDERS as p (p.id)}
                <li>
                  <button
                    type="button"
                    class="provider"
                    class:active={editingProvider === p.id}
                    onclick={() => (editingProvider = p.id)}
                  >
                    <span class="text">
                      <span class="title">
                        {p.label}
                        {#if config.agents.chat_provider === p.id}<span class="role-badge">Chat</span>{/if}
                        {#if config.agents.skill_provider === p.id}<span class="role-badge">Skills</span>{/if}
                      </span>
                      <span class="meta">{p.blurb}</span>
                    </span>
                  </button>
                </li>
              {/each}
            </ul>

            <!--
              Bindings target the concrete `config.agents.providers[<id>]`
              path, never the `editingSettings` `$derived` — a `bind:value` to a
              property of a derived is silently dropped on the next recompute
              in Svelte 5, which quietly wiped provider fields from
              config.toml. `config` is `$state`, so the computed-member path
              below is a real two-way target and retargets when the list
              changes `editingProvider`.
            -->
            {#if config.agents.providers[editingProvider]}
              {@const pid = editingProvider}
              <div class="provider-form">
                {#if editingMeta.key === "none"}
                  <p class="hint">
                    Kein Schlüssel oder Basis-URL nötig — Anthropic wird über den CLI-Login des Abos abgerechnet.
                  </p>
                {:else}
                  <label class="field">
                    <span>Basis-URL</span>
                    <input type="text" spellcheck="false" bind:value={config.agents.providers[pid].base_url} placeholder="https://…" />
                  </label>
                  <label class="field">
                    <span>API-Schlüssel {editingMeta.key === "optional" ? "(optional)" : ""}</span>
                    <input
                      type="password"
                      autocomplete="off"
                      spellcheck="false"
                      value={keyEdits[pid] ?? ""}
                      oninput={(e) => setKeyEdit(pid, e.currentTarget.value)}
                      placeholder={config.agents.providers[pid].has_key
                        ? "•••••• gespeichert — leer lassen zum Behalten"
                        : editingMeta.key === "optional"
                          ? "Platzhalter genügt für lokales Ollama"
                          : "erforderlich"}
                    />
                    {#if config.agents.providers[pid].has_key && (keyEdits[pid] ?? "") === "" && pid in keyEdits}
                      <span class="hint">Schlüssel wird beim Speichern gelöscht.</span>
                    {/if}
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
            {:else}
              <p class="status error">
                Unbekannter Provider „{editingProvider}“ — keine
                Einstellungen zum Bearbeiten. Wähle oben einen der bekannten Provider.
              </p>
            {/if}

            <div class="actions">
              <button type="button" disabled={savingProvider} onclick={saveProvider}>
                {savingProvider ? "Speichern…" : "Provider speichern"}
              </button>
            </div>
          </section>

          <section>
            <h3>Provider-Zuordnung</h3>
            <p class="lead">
              Welcher Provider den interaktiven Chat bedient und welcher die Skills &amp; Routinen — das dürfen
              verschiedene sein, z. B. Anthropic für den Chat und lokales Ollama für die Digests.
            </p>
            <div class="role-picks">
              <label class="field">
                <span>Provider für Chat</span>
                <select bind:value={config.agents.chat_provider}>
                  {#each PROVIDERS as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
                </select>
              </label>
              <label class="field">
                <span>Provider für Skills &amp; Routinen</span>
                <select bind:value={config.agents.skill_provider}>
                  {#each PROVIDERS as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
                </select>
              </label>
            </div>

            <div class="provider-form spend-form">
              {#each spend as s (s.role)}
                {#if s.metered}
                  <p class="hint">
                    Ausgaben <strong>{s.role} → {s.provider}</strong>:
                    <strong>${s.today_usd.toFixed(4)}</strong> heute{#if s.daily_cap_usd != null}
                      &nbsp;/&nbsp;${s.daily_cap_usd.toFixed(2)} Limit{/if}
                    &nbsp;·&nbsp;${s.month_usd.toFixed(2)} diesen Monat
                  </p>
                {:else}
                  <p class="hint">{s.role} → {s.provider} wird übers Abo abgerechnet — keine Kostenerfassung.</p>
                {/if}
              {/each}
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
            </div>

            <div class="actions">
              <button type="button" disabled={savingProvider} onclick={saveProvider}>
                {savingProvider ? "Speichern…" : "Zuordnung speichern"}
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
  .field select {
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }
  .field select:focus {
    outline: none;
    border-color: var(--ax-accent);
  }

  .role-picks {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: var(--ax-space-2);
    margin-bottom: var(--ax-space-3);
  }
  .role-badge {
    margin-left: var(--ax-space-1);
    padding: 0 var(--ax-space-1);
    font-size: var(--ax-font-size-xs);
    color: var(--ax-accent);
    background: var(--ax-accent-muted);
    border-radius: var(--ax-radius-sm);
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

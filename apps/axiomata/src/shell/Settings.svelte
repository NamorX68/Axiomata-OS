<!--
  Settings dialog, in four tabs (chrome is the shared `Window.svelte`):
  Darstellung (theme, canvas options, window-transparency slider, custom
  CSS), Workspace (vault path), KI-Provider (provider config + role
  assignment + spend), Über (read-only app facts). Was one long scrolling
  body before this — split into tabs once it grew past ~9 sections.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import {
    type AppInfo,
    type ClaudeLoginStatus,
    type ConfigUpdate,
    type ConfigView,
    type KeyUpdate,
    type HarnessId,
    type ProviderId,
    type SpendSummary,
    invokeBackend,
  } from "../core/backend";
  import { HARNESSES, providerAfterHarnessChange, providersFor, usesClaudeCode } from "../core/harness";
  import { WEB_KEYS, type WebKeyId, webUpdate } from "../core/webSettings";
  import { brainView, SHINES, SPEED_MAX } from "../core/brainView";
  import { MOTION_LEVELS, isMotionLevel, motion, motionSetting, systemReducesMotion, turns } from "../core/motion";
  import { effects } from "../core/effects";
  import { CUE_LABELS, CUE_ORDER } from "../core/sounds";
  import Icon from "../ui/Icon.svelte";
  import ModelPicker from "../ui/ModelPicker.svelte";
  import { loadModelCatalog, modelCatalog, providerOptions, refreshModelCatalog } from "../core/modelCatalog";
  import { playCue, unlockAudio } from "./soundPlayer";
  import { UI_FONTS, uiFont } from "../core/uiFont";
  import { customTheme, loadCustomTheme } from "../core/custom-theme";
  import { activeTheme, showGrid, snapEdges, windowTransparency } from "../core/stores";
  import { KEYBOARD_SETTINGS, isKeyboardSetting, keyboardSetting, platform, platformLabel } from "../core/keys";
  import { UI_SIZE_MAX, UI_SIZE_MIN, uiScaleAuto, uiSize } from "../core/uiScale";
  import { THEMES, applyTheme } from "../core/themes";
  import ThemePreview from "../ui/ThemePreview.svelte";
  import { toast } from "../core/toast";
  import { TEMPLATE } from "../theme/validator";
  import Window from "./Window.svelte";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  type TabId = "appearance" | "workspace" | "providers" | "about";
  const TABS: { id: TabId; label: string }[] = [
    { id: "appearance", label: "Appearance" },
    { id: "workspace", label: "Workspace" },
    { id: "providers", label: "AI providers" },
    { id: "about", label: "About" },
  ];
  let tab = $state<TabId>("appearance");

  // What "Auto" comes to here, and the slider's value while it is dragged (it follows the setting otherwise).
  const autoPercent = $derived(Math.round($uiScaleAuto * 100));
  let sizeDraft = $state(100);
  $effect(() => {
    sizeDraft = $uiSize === "auto" ? autoPercent : $uiSize;
  });

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
  /** Web-search key edits, as `keyEdits` for the providers: absent = keep, `""` = clear, a value = set. */
  let webKeyEdits = $state<Partial<Record<WebKeyId, string>>>({});
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
    { id: "anthropic", label: "Anthropic", blurb: "Billed through the subscription — no key", key: "none" },
    { id: "open_router", label: "OpenRouter", blurb: "Anthropic-kompatibler Endpoint", key: "required" },
    { id: "ollama", label: "Ollama", blurb: "Local model on this machine", key: "optional" },
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
        chat_harness: c.agents.chat_harness,
        skill_harness: c.agents.skill_harness,
        daily_usd_cap: c.agents.daily_usd_cap,
        web: webUpdate(c.agents.web, webKeyEdits),
      },
    };
  }

  /** A role's harness changed: its provider follows when the new harness cannot serve it (the matrix of
   *  `core/harness.ts`; Rust's `validate_for_save` refuses the pair otherwise). */
  function setHarness(role: "chat" | "skill", harness: HarnessId) {
    if (!config) return;
    const order = PROVIDERS.map((p) => p.id);
    if (role === "chat") {
      config.agents.chat_harness = harness;
      config.agents.chat_provider = providerAfterHarnessChange(harness, config.agents.chat_provider, order);
    } else {
      config.agents.skill_harness = harness;
      config.agents.skill_provider = providerAfterHarnessChange(harness, config.agents.skill_provider, order);
    }
    if (harness === "claude_code" && !claudeLogin) void refreshClaudeLogin();
  }

  /** The sign-in of the app's own Claude Code (harness-choice D7): its state, a running sign-in, a code for it. */
  let claudeLogin = $state<ClaudeLoginStatus | null>(null);
  let claudeLoginError = $state<string | null>(null);
  let signingIn = $state(false);
  let signInCode = $state("");
  const needsClaude = $derived(
    config ? usesClaudeCode(config.agents.chat_harness, config.agents.skill_harness) : false,
  );

  async function refreshClaudeLogin() {
    try {
      claudeLogin = await invokeBackend<ClaudeLoginStatus>("claude_login_status");
      claudeLoginError = null;
    } catch (e) {
      claudeLogin = null;
      claudeLoginError = String(e);
    }
  }

  async function signIn() {
    signingIn = true;
    signInCode = "";
    try {
      claudeLogin = await invokeBackend<ClaudeLoginStatus>("claude_sign_in");
      claudeLoginError = null;
      toast(claudeLogin.logged_in ? "Claude Code is signed in." : "The sign-in did not complete.", "info");
    } catch (e) {
      toast(`Sign-in failed: ${e}`, "warning");
    } finally {
      signingIn = false;
    }
  }

  async function sendSignInCode() {
    const code = signInCode.trim();
    if (!code) return;
    try {
      await invokeBackend<void>("claude_sign_in_code", { code });
      signInCode = "";
    } catch (e) {
      toast(`The code was not taken: ${e}`, "warning");
    }
  }

  async function signOut() {
    try {
      claudeLogin = await invokeBackend<ClaudeLoginStatus>("claude_sign_out");
      toast("Claude Code is signed out.", "info");
    } catch (e) {
      toast(`Sign-out failed: ${e}`, "warning");
    }
  }

  /** `save_config` returns `true` when only a restart will pick up the new
   *  workspace root (everything else applies live). */
  async function persist(): Promise<boolean> {
    if (!config) return false;
    return invokeBackend<boolean>("save_config", { newConfig: buildUpdate(config) });
  }

  async function saveVault() {
    if (!config?.workspace_root.trim()) {
      toast("The vault path must not be empty.", "warning");
      return;
    }
    savingVault = true;
    try {
      const restartNeeded = await persist();
      toast(
        restartNeeded
          ? "Vault path saved — restart Axiomata-OS to switch."
          : "Vault path saved.",
        "info",
      );
    } catch (e) {
      toast(`Saving failed: ${e}`, "warning");
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
    if (!s) return `${roleLabel}: provider “${id}” has no settings.`;
    const hasKey = s.has_key || keyUpdateFor(id).kind === "set";
    if (meta.key === "required" && !hasKey) return `${roleLabel} (${meta.label}) needs an API key.`;

    const v = (modelLabel === "chat_model" ? s.chat_model : s.skill_model).trim();
    const human = modelLabel === "chat_model" ? "Chat-Modell" : "Skills-Modell";
    if (!v) return `${roleLabel} (${meta.label}): ${human} must not be empty.`;
    if (/\s/.test(v)) return `${roleLabel} (${meta.label}): ${human} contains spaces.`;
    const opens = (v.match(/\[/g) ?? []).length;
    const closes = (v.match(/\]/g) ?? []).length;
    if (opens !== closes || /[()]/.test(v)) {
      return `${roleLabel} (${meta.label}): ${human} has unbalanced brackets — “${v}”.`;
    }
    const url = s.base_url?.trim();
    if (!url || !/^https:\/\/.+/.test(url)) {
      const loopback = /^http:\/\/(localhost|127\.0\.0\.1|\[::1\])(:\d+)?(\/|$)/.test(url ?? "");
      if (!loopback) return `${roleLabel} (${meta.label}): Base URL must start with https://.`;
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
      toast("Provider settings saved — they apply from the next agent call.", "info");
      keyEdits = {};
      webKeyEdits = {};
      await loadConfig();
      await refreshSpend();
    } catch (e) {
      toast(`Saving failed: ${e}`, "warning");
    } finally {
      savingProvider = false;
    }
  }

  /** The provider whose model list is being fetched again, if any. */
  let refreshingModels = $state<ProviderId | null>(null);

  /** The line under a provider's model fields: where its list comes from and how fresh it is. */
  function catalogStatus(id: ProviderId): string {
    if (id === "anthropic") return "Built-in list — Anthropic has no model list to ask without an API key.";
    const entry = $modelCatalog?.providers[id];
    if (!entry) return "No model list yet.";
    if (entry.error) return `The last fetch failed: ${entry.error}`;
    const when = entry.fetched_at ? new Date(entry.fetched_at).toLocaleString() : "never";
    return `${entry.models.length} models, fetched ${when}; refreshed at start and once a day.`;
  }

  async function refreshModels(id: ProviderId) {
    refreshingModels = id;
    try {
      const outcome = await refreshModelCatalog(id);
      if (outcome.price_error) toast(`The prices could not be saved: ${outcome.price_error}`, "danger");
      else if (outcome.repriced.length > 0) toast(`Prices updated: ${outcome.repriced.join(", ")}`, "info");
    } catch (err) {
      toast(`Could not fetch the model list: ${String(err)}`, "danger");
    } finally {
      refreshingModels = null;
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

  onMount(async () => {
    try {
      info = await invokeBackend<AppInfo>("get_app_info");
    } catch {
      info = null;
    }
    await loadConfig();
    void loadModelCatalog().catch(() => modelCatalog.set(null));
    await refreshSpend();
    if (needsClaude) await refreshClaudeLogin();
  });
</script>

{#if open}
  <Window
    title="Settings"
    onClose={() => (open = false)}
    style="width: min(calc(620px * var(--ax-ui-scale)), calc(100vw - 2 * var(--ax-space-5))); max-height: calc(100vh - 2 * var(--ax-space-5));"
  >
    <nav class="tabs">
      {#each TABS as t (t.id)}
        <button type="button" class="tab" class:active={tab === t.id} onclick={() => (tab = t.id)}>
          {t.label}
        </button>
      {/each}
    </nav>

    <div class="body">
      {#if tab === "appearance"}
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
                  <ThemePreview theme={t.id} />
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
          <h3>Typeface</h3>
          <div class="fonts" role="radiogroup" aria-label="Typeface">
            {#each UI_FONTS as f (f.id)}
              <button
                type="button"
                role="radio"
                class="font"
                class:active={$uiFont === f.id}
                aria-checked={$uiFont === f.id}
                style:font-family="var(--ax-font-{f.id})"
                onclick={() => uiFont.set(f.id)}
              >
                <span class="sample">Aa</span>
                <span class="name">{f.label}</span>
              </button>
            {/each}
          </div>
          <p class="hint">For the whole interface. Code in the editor and the terminal keep their own fonts.</p>
        </section>

        <section>
          <h3>Motion</h3>
          <label class="row">
            <span class="label">Movement</span>
            <select
              value={$motionSetting}
              onchange={(e) => {
                const v = e.currentTarget.value;
                if (isMotionLevel(v)) motionSetting.set(v);
              }}
            >
              {#each MOTION_LEVELS as level (level.id)}
                <option value={level.id}>{level.label}</option>
              {/each}
            </select>
          </label>
          <p class="hint">
            Full: the glow breathes, sparks fly, the Orbit turns. Calm: the glow holds still, nothing flies, the Orbit
            still turns. Off: nothing moves by itself.
            {#if $systemReducesMotion}<strong>macOS “Reduce motion” is on, so motion is off.</strong>{/if}
          </p>
        </section>

        <section>
          <h3>Light</h3>
          <label class="opt">
            <input type="checkbox" bind:checked={$effects.pointerLight} /> Light under the pointer
            <span class="hint">(the hex lines glow where the mouse is)</span>
          </label>
          <label class="opt">
            <input type="checkbox" bind:checked={$effects.dayLight} /> Time-of-day light
            <span class="hint">(a faint light that moves with the sun)</span>
          </label>
          <label class="opt">
            <input type="checkbox" bind:checked={$effects.ignition} /> Start ignition
            <span class="hint">(the logo lights up when the app starts; motion Full only)</span>
          </label>
        </section>

        <section>
          <h3>Sound</h3>
          <label class="opt">
            <input
              type="checkbox"
              bind:checked={$effects.sounds}
              onchange={() => {
                if ($effects.sounds) void unlockAudio();
              }}
            />
            Sounds
            <span class="hint">(only while Axiomata is in the background)</span>
          </label>
          <div class="cues">
            {#each CUE_ORDER as cue (cue)}
              <button type="button" class="cue" onclick={() => void unlockAudio().then(() => playCue(cue))}>
                <Icon name="play" size="sm" />{CUE_LABELS[cue]}
              </button>
            {/each}
          </div>
        </section>

        <section>
          <h3>Canvas</h3>
          <label class="opt"><input type="checkbox" bind:checked={$showGrid} /> Show dot grid <span class="hint">(tiles snap to it either way)</span></label>
          <label class="opt"><input type="checkbox" bind:checked={$snapEdges} /> Magnetic edges <span class="hint">(tiles stick to their neighbours within 8 px)</span></label>
        </section>

        <section>
          <h3>Orbit &amp; Second Brain</h3>
          <label class="opt">
            Speed
            <input class="speed" type="range" min="0.005" max={SPEED_MAX} step="0.005" bind:value={$brainView.speed} disabled={!turns($motion)} />
          </label>
          <label class="opt"><input type="checkbox" bind:checked={$brainView.labels} /> Labels <span class="hint">(skills, areas and the hub)</span></label>
          <label class="opt"><input type="checkbox" bind:checked={$brainView.fileNames} /> File names <span class="hint">(next to every note in the Second Brain)</span></label>
          <label class="row">
            <span class="label">Shimmer</span>
            <select bind:value={$brainView.shine}>
              {#each SHINES as s (s.id)}
                <option value={s.id}>{s.label}</option>
              {/each}
            </select>
            <span class="hint">(how the hex cells catch the light; still unless motion is Full)</span>
          </label>
        </section>

        <section>
          <h3>UI size</h3>
          <label class="opt">
            <input
              type="checkbox"
              checked={$uiSize === "auto"}
              onchange={(e) => uiSize.set(e.currentTarget.checked ? "auto" : autoPercent)}
            />
            Auto <span class="hint">({autoPercent}% on this display)</span>
          </label>
          <label class="row">
            <span class="label">Text and controls</span>
            <!-- Taken over on release, not while dragging: the dialog itself grows with the size, and a slider that
                 moves under the pointer cannot be dragged. -->
            <input
              type="range"
              min={UI_SIZE_MIN}
              max={UI_SIZE_MAX}
              step="1"
              value={sizeDraft}
              oninput={(e) => (sizeDraft = Number(e.currentTarget.value))}
              onchange={(e) => uiSize.set(Number(e.currentTarget.value))}
            />
            <span class="readout">{sizeDraft}%</span>
          </label>
          <p class="hint">
            “Auto” follows the real pixel density of the display the window is on: on a dense monitor the interface
            grows so it reads as well as on a MacBook. The fonts of the editor and the terminal are not affected.
          </p>
        </section>

        <section>
          <h3>Keyboard</h3>
          <label class="row">
            <span class="label">Shortcuts follow</span>
            <select
              value={$keyboardSetting}
              onchange={(e) => {
                const v = e.currentTarget.value;
                if (isKeyboardSetting(v)) keyboardSetting.set(v);
              }}
            >
              {#each KEYBOARD_SETTINGS as option (option.value)}
                <option value={option.value}>
                  {option.label}{option.value === "auto" ? ` (${platformLabel($platform)})` : ""}
                </option>
              {/each}
            </select>
          </label>
          <p class="hint">
            Mac: ⌘ is the main key. Windows &amp; Linux: Ctrl is, and in a terminal an app shortcut takes Ctrl+Shift,
            because Ctrl belongs to the shell there. "Auto" follows this machine; the other two let you try the other
            keys here.
          </p>
        </section>

        <section>
          <h3>Transparency</h3>
          <label class="row">
            <span class="label">Window transparency</span>
            <input type="range" min="0" max="100" step="5" bind:value={$windowTransparency} />
            <span class="readout">{$windowTransparency}%</span>
          </label>
          <p class="hint">
            How see-through windows, dialogs and dragged tiles look — 0% an opaque surface,
            100% fully transparent, 50% the unchanged default look of the theme.
          </p>
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
      {:else if tab === "workspace"}
        {#if config}
          <section>
            <h3>Vault</h3>
            <p class="lead">Root folder of the Second Brain workspace. A change takes effect after a restart.</p>
            <label class="field">
              <span>Path</span>
              <input type="text" spellcheck="false" bind:value={config.workspace_root} placeholder="/Users/…/Vault" />
            </label>
            <div class="actions">
              <button type="button" disabled={savingVault} onclick={saveVault}>
                {savingVault ? "Saving…" : "Save & restart"}
              </button>
            </div>
          </section>
        {/if}
      {:else if tab === "providers"}
        {#if config}
          <section>
            <h3>Edit providers</h3>
            <p class="lead">
              A provider is where the model runs — its endpoint, key and models. Every provider is configured
              independently here; which harness and provider serve the chat and the skills is set in the next section.
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
                    <span>Base URL</span>
                    <input type="text" spellcheck="false" bind:value={config.agents.providers[pid].base_url} placeholder="https://…" />
                  </label>
                  <label class="field">
                    <span>API key {editingMeta.key === "optional" ? "(optional)" : ""}</span>
                    <input
                      type="password"
                      autocomplete="off"
                      spellcheck="false"
                      value={keyEdits[pid] ?? ""}
                      oninput={(e) => setKeyEdit(pid, e.currentTarget.value)}
                      placeholder={config.agents.providers[pid].has_key
                        ? "•••••• saved — leave empty to keep"
                        : editingMeta.key === "optional"
                          ? "A placeholder is enough for a local Ollama"
                          : "required"}
                    />
                    {#if config.agents.providers[pid].has_key && (keyEdits[pid] ?? "") === "" && pid in keyEdits}
                      <span class="hint">The key is deleted on save.</span>
                    {/if}
                  </label>
                {/if}
                <div class="field">
                  <span>Chat model</span>
                  <ModelPicker
                    label="Chat model"
                    bind:value={config.agents.providers[pid].chat_model}
                    options={providerOptions($modelCatalog, pid)}
                    placeholder="e.g. claude-sonnet-5"
                  />
                </div>
                <div class="field">
                  <span>Skills/routines model</span>
                  <ModelPicker
                    label="Skills/routines model"
                    bind:value={config.agents.providers[pid].skill_model}
                    options={providerOptions($modelCatalog, pid)}
                    placeholder="e.g. claude-haiku-5-5"
                    onRefresh={pid === "anthropic" ? undefined : () => void refreshModels(pid)}
                    refreshing={refreshingModels === pid}
                    status={catalogStatus(pid)}
                  />
                </div>
                <p class="hint">Ein <code>model:</code> in the SKILL.md frontmatter still overrides the skills model.</p>
              </div>
            {:else}
              <p class="status error">
                Unbekannter Provider „{editingProvider}“ — keine
                Einstellungen zum Bearbeiten. Wähle oben einen der bekannten Provider.
              </p>
            {/if}

            <div class="actions">
              <button type="button" disabled={savingProvider} onclick={saveProvider}>
                {savingProvider ? "Saving…" : "Save provider"}
              </button>
            </div>
          </section>

          <section>
            <h3>Harness &amp; provider assignment</h3>
            <p class="lead">
              The harness drives the agent — its tools, rights and loop; the provider serves the model. Each role picks
              both, e.g. Claude Code for the chat and Glut with a local Ollama for the digests. A skill's own
              <code>backend:</code> still wins over the harness set here.
            </p>
            <div class="role-picks">
              {#each [{ role: "chat", label: "chat" }, { role: "skill", label: "skills & routines" }] as r (r.role)}
                {@const harness = r.role === "chat" ? config.agents.chat_harness : config.agents.skill_harness}
                <div class="role-pick">
                  <label class="field">
                    <span>Harness for {r.label}</span>
                    <select
                      value={harness}
                      onchange={(e) => setHarness(r.role as "chat" | "skill", e.currentTarget.value as HarnessId)}
                    >
                      {#each HARNESSES as h (h.id)}<option value={h.id}>{h.label}</option>{/each}
                    </select>
                  </label>
                  <label class="field">
                    <span>Provider for {r.label}</span>
                    {#if r.role === "chat"}
                      <select bind:value={config.agents.chat_provider}>
                        {#each providersFor(harness, PROVIDERS) as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
                      </select>
                    {:else}
                      <select bind:value={config.agents.skill_provider}>
                        {#each providersFor(harness, PROVIDERS) as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
                      </select>
                    {/if}
                  </label>
                  <p class="hint">{HARNESSES.find((h) => h.id === harness)?.blurb}</p>
                </div>
              {/each}
            </div>
            {#if config.agents.skill_harness === "axh"}
              <p class="hint">
                A skill that reads the web (<code>news-web</code>) uses Glut's web search — its keys and limits are in
                the Web search block below.
              </p>
            {/if}

            {#if needsClaude}
              <div class="provider-form claude-login">
                <span class="title">Claude Code sign-in</span>
                {#if claudeLogin?.logged_in}
                  <p class="status applied">
                    Signed in as {claudeLogin.email ?? "unknown"}{#if claudeLogin.subscription}
                      &nbsp;({claudeLogin.subscription}){/if} — the app's own login, apart from yours in
                    <code>~/.claude</code>.
                  </p>
                  <div class="actions">
                    <button type="button" onclick={signOut}>Sign out</button>
                  </div>
                {:else}
                  <p class="status error">
                    {#if claudeLoginError}
                      Claude Code cannot be asked: {claudeLoginError}
                    {:else}
                      Not signed in — chat and skills on Claude Code fail until the app's Claude Code signs in.
                    {/if}
                  </p>
                  <div class="actions">
                    <button type="button" disabled={signingIn} onclick={signIn}>
                      {signingIn ? "Waiting for the browser…" : "Sign in with Anthropic"}
                    </button>
                  </div>
                  {#if signingIn}
                    <label class="field">
                      <span>Code (only if the browser page shows one)</span>
                      <span class="code-row">
                        <input type="text" spellcheck="false" autocomplete="off" bind:value={signInCode} />
                        <button type="button" disabled={!signInCode.trim()} onclick={sendSignInCode}>Send</button>
                      </span>
                    </label>
                  {/if}
                {/if}
              </div>
            {/if}

            <div class="provider-form web-search">
              <span class="title">Web search (Glut)</span>
              <p class="hint">
                Glut's <code>web_search</code> asks Exa first, then TinyFish, then Tavily — a provider without its key is
                skipped. Keys are stored in <code>config.toml</code> and never shown again.
              </p>
              {#each WEB_KEYS as k (k.id)}
                {@const stored = k.stored(config.agents.web)}
                <label class="field">
                  <span>{k.label} key</span>
                  <input
                    type="password"
                    autocomplete="off"
                    spellcheck="false"
                    value={webKeyEdits[k.id] ?? ""}
                    oninput={(e) => (webKeyEdits = { ...webKeyEdits, [k.id]: e.currentTarget.value })}
                    placeholder={stored ? "•••••• saved — leave empty to keep" : k.id === "exa" ? "optional" : "not set"}
                  />
                  <span class="hint">{k.hint}</span>
                  {#if stored && (webKeyEdits[k.id] ?? "") === "" && k.id in webKeyEdits}
                    <span class="hint">The key is deleted on save.</span>
                  {/if}
                </label>
              {/each}
              <div class="role-picks">
                <label class="field">
                  <span>Searches per day (all providers)</span>
                  <input type="number" min="0" step="1" bind:value={config.agents.web.daily_search_limit} />
                </label>
                <label class="field">
                  <span>Tavily searches per month</span>
                  <input type="number" min="0" step="1" bind:value={config.agents.web.tavily_monthly_limit} />
                </label>
              </div>
              <p class="hint">0 turns the search (or Tavily) off. 1,000 Tavily searches a month are its free credits.</p>
            </div>

            <div class="provider-form spend-form">
              {#each spend as s (s.role)}
                {#if s.metered}
                  <p class="hint">
                    Spend <strong>{s.role} → {s.provider}</strong>:
                    <strong>${s.today_usd.toFixed(4)}</strong> today{#if s.daily_cap_usd != null}
                      &nbsp;/&nbsp;${s.daily_cap_usd.toFixed(2)} limit{/if}
                    &nbsp;·&nbsp;${s.month_usd.toFixed(2)} this month
                  </p>
                {:else}
                  <p class="hint">{s.role} → {s.provider} is billed through the subscription — no cost tracking.</p>
                {/if}
              {/each}
              <label class="field">
                <span>Daily limit (USD, paid providers)</span>
                <input
                  type="number"
                  min="0"
                  step="0.5"
                  placeholder="empty = no limit"
                  value={config.agents.daily_usd_cap ?? ""}
                  oninput={(e) => setDailyCap(e.currentTarget.value)}
                />
              </label>
              <p class="hint">Once today's total reaches the limit, the next agent call through a paid provider is refused.</p>
            </div>

            <div class="actions">
              <button type="button" disabled={savingProvider} onclick={saveProvider}>
                {savingProvider ? "Saving…" : "Save assignment"}
              </button>
            </div>
          </section>
        {/if}
      {:else if tab === "about"}
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
      {/if}
    </div>
  </Window>
{/if}

<style>
  .tabs {
    display: flex;
    gap: var(--ax-space-1);
    padding: 0 var(--ax-space-4);
    border-bottom: 1px solid var(--ax-border);
    flex: 0 0 auto;
  }
  .tab {
    padding: var(--ax-space-2) var(--ax-space-3);
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    color: var(--ax-text-muted);
    font: var(--ax-type-body);
    font-weight: 500;
  }
  .tab:hover:not(.active) {
    color: var(--ax-text);
  }
  /* Selected, not alive (UX2): the text colour, never the accent. */
  .tab.active {
    color: var(--ax-text);
    border-bottom-color: var(--ax-text);
  }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    padding: var(--ax-space-3) var(--ax-space-4) var(--ax-space-4);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-6);
  }
  /* Section heads in the label role (UX2, as `ui/SectionHead.svelte`): quiet, so the choices below them speak. */
  h3 {
    font: var(--ax-type-label);
    letter-spacing: var(--ax-tracking-label);
    text-transform: uppercase;
    color: var(--ax-text-muted);
    margin-bottom: var(--ax-space-2);
  }

  .themes {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(calc(170px * var(--ax-ui-scale)), 1fr));
    gap: var(--ax-space-2);
  }
  /* A theme as a real piece of the app (UX7b, G21), its name below; a click switches, hovering recolours nothing. */
  .theme {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: var(--ax-space-2);
    padding: var(--ax-space-2);
    text-align: left;
    background: var(--ax-surface-2);
  }
  /* The theme in use is chosen, not alive: neutral (the accent rule). */
  .theme.active {
    border-color: var(--ax-text-muted);
    background: var(--ax-selected-bg);
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

  .fonts {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--ax-space-2);
    margin-bottom: var(--ax-space-2);
  }
  .font {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--ax-space-1);
    padding: var(--ax-space-3);
    text-align: left;
    background: var(--ax-surface-2);
  }
  /* Chosen, not alive: neutral like the theme cards (the accent rule). */
  .font.active {
    border-color: var(--ax-text-muted);
    background: var(--ax-selected-bg);
  }
  .font .sample {
    font-size: var(--ax-font-size-display);
    font-weight: 600;
    line-height: 1;
    letter-spacing: -0.02em;
  }
  .font .name {
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
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
  .cues {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-2);
    margin-top: var(--ax-space-1);
  }
  .cue {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    font-size: var(--ax-font-size-sm);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    margin-bottom: var(--ax-space-1);
  }
  .row .label {
    flex: 0 0 auto;
  }
  .row input[type="range"] {
    flex: 1 1 auto;
    accent-color: var(--ax-accent);
  }
  .readout {
    min-width: calc(40px * var(--ax-ui-scale));
    text-align: right;
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
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
    grid-template-columns: repeat(auto-fit, minmax(calc(200px * var(--ax-ui-scale)), 1fr));
    gap: var(--ax-space-2);
    margin-bottom: var(--ax-space-3);
  }
  .role-pick {
    display: flex;
    flex-direction: column;
  }
  .web-search .title,
  .claude-login .title {
    display: block;
    margin-bottom: var(--ax-space-2);
    font-weight: 600;
  }
  .code-row {
    display: flex;
    gap: var(--ax-space-2);
  }
  .code-row input {
    flex: 1;
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
    grid-template-columns: repeat(auto-fill, minmax(calc(170px * var(--ax-ui-scale)), 1fr));
    gap: var(--ax-space-2);
  }
  .provider {
    width: 100%;
    padding: var(--ax-space-2);
    text-align: left;
    background: var(--ax-surface-2);
  }
  /* Chosen, not alive: neutral like the theme cards (the accent rule). */
  .provider.active {
    border-color: var(--ax-text-muted);
    background: var(--ax-selected-bg);
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
  .speed {
    accent-color: var(--ax-accent);
  }
</style>

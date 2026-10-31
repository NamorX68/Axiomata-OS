<!--
  Settings dialog, in tabs (chrome is the shared `Window.svelte`):
  Appearance (theme, canvas options, window-transparency slider, custom
  CSS), Workspace (vault path), AI providers (provider config + role
  assignment + spend), Storage, About (read-only app facts). Was one long scrolling
  body before this — split into tabs once it grew past ~9 sections. Every text comes from `i18n/settings.ts`.
-->
<script lang="ts">
  import { messageOf } from "../core/errors";
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
    CONFIG_SAVED,
    invokeBackend,
    isEffortLevel,
  } from "../core/backend";
  import { formatDateTime, formatNumber, formatUsd } from "../core/format";
  import { tFormat } from "../i18n/format";
  import { tSettings } from "../i18n/settings";
  import LanguageSetting from "./LanguageSetting.svelte";
  import { emit } from "../core/bus";
  import { registerShellView } from "../core/openViews";
  import { HARNESSES, providerAfterHarnessChange, providersFor, usesClaudeCode } from "../core/harness";
  import { WEB_KEYS, type WebKeyId, webUpdate } from "../core/webSettings";
  import { BRAIN_LAYOUTS, brainView, SHINES, SPEED_MAX } from "../core/brainView";
  import { MOTION_LEVELS, isMotionLevel, motion, motionSetting, systemReducesMotion, turns } from "../core/motion";
  import { effects } from "../core/effects";
  import { CUE_LABELS, CUE_ORDER } from "../core/sounds";
  import Icon from "../ui/Icon.svelte";
  import ModelPicker from "../ui/ModelPicker.svelte";
  import { loadModelCatalog, modelCatalog, providerOptions, refreshModelCatalog } from "../core/modelCatalog";
  import { offeredLevels, reasoningOf, takesLevel, unlistedNote } from "../core/effortLevels";
  import { playCue, unlockAudio } from "./soundPlayer";
  import { UI_FONTS, uiFont } from "../core/uiFont";
  import { customTheme, loadCustomTheme } from "../core/custom-theme";
  import { activeTheme, glassBlur, glassDim, showGrid, snapEdges, windowTransparency } from "../core/stores";
  import { MAX_GROUND_BLUR, MAX_GROUND_DIM, systemReducesTransparency } from "../core/glass";
  import { clearWallpaper, pickWallpaper, wallpaper } from "../core/wallpaper";
  import { KEYBOARD_SETTINGS, isKeyboardSetting, keyboardSetting, platform, platformLabel } from "../core/keys";
  import { UI_SIZE_MAX, UI_SIZE_MIN, uiScaleAuto, uiSize } from "../core/uiScale";
  import { THEMES, applyTheme } from "../core/themes";
  import ThemePreview from "../ui/ThemePreview.svelte";
  import { toast } from "../core/toast";
  import { TEMPLATE } from "../theme/validator";
  import StorageSettings from "./StorageSettings.svelte";
  import TagsSettings from "./TagsSettings.svelte";
  import { tagProposals, waitingLabel } from "../core/tagProposals";
  import HexCount from "../ui/HexCount.svelte";
  import Window from "./Window.svelte";
  import { SETTINGS_TABS, type SettingsTabId } from "../core/settingsTabs";

  let { open = $bindable(false), initialTab = null }: { open?: boolean; initialTab?: string | null } = $props();

  type TabId = SettingsTabId;
  const TABS = SETTINGS_TABS;
  let tab = $state<TabId>("appearance");
  // Opened for one place (the Dock's tag badge opens Workspace): that tab, each time it is opened so.
  // Known as open to the chat's `close_view` (CH5). Closing loses nothing: this component stays mounted while the
  // window is closed, so a field typed but not saved is still there when it opens again.
  $effect(() => (open ? registerShellView("settings", { title: "Settings", close: () => (open = false) }) : undefined));

  $effect(() => {
    const wanted = TABS.find((t) => t.id === initialTab);
    if (open && wanted) tab = wanted.id;
  });

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
  const PROVIDERS: { id: ProviderId; label: string; blurb: () => string; key: "none" | "required" | "optional" }[] = [
    { id: "anthropic", label: "Anthropic", blurb: () => tSettings("blurbAnthropic"), key: "none" },
    { id: "open_router", label: "OpenRouter", blurb: () => tSettings("blurbOpenRouter"), key: "required" },
    { id: "ollama", label: "Ollama", blurb: () => tSettings("blurbOllama"), key: "optional" },
  ];

  /** A dollar amount with a fixed number of decimals, in the app's language. */
  function usd(amount: number, decimals: number): string {
    return formatUsd(amount, decimals);
  }

  /** The two roles of the harness/provider assignment; a label is read when drawn, so it follows the language. */
  const ROLE_PICKS: readonly { role: "chat" | "skill"; label: () => string }[] = [
    { role: "chat", label: () => tSettings("forChat") },
    { role: "skill", label: () => tSettings("forSkills") },
  ];

  /** The text of a caught error, for a toast. */
  const errorText = (err: unknown): string => messageOf(err);

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
    toast(state.message(), state.status === "applied" || state.status === "absent" ? "info" : "warning");
  }

  let wallpaperBusy = $state(false);

  /** Runs a background-image step (docs/plans/glass.md, G15); a refusal says why. */
  async function wallpaperStep(
    work: () => Promise<unknown>,
    failed: "imageNotTaken" | "imageNotRemoved",
  ): Promise<void> {
    wallpaperBusy = true;
    try {
      await work();
    } catch (err) {
      toast(tSettings(failed, { error: errorText(err) }), "warning");
    } finally {
      wallpaperBusy = false;
    }
  }

  async function copyTemplate() {
    try {
      await navigator.clipboard.writeText(TEMPLATE);
      toast(tSettings("templateCopied"));
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
        chat_effort: c.agents.chat_effort,
        skill_effort: c.agents.skill_effort,
        daily_usd_cap: c.agents.daily_usd_cap,
        web: webUpdate(c.agents.web, webKeyEdits),
      },
    };
  }

  /** A role's Glut effort picked; `""` is Glut's own setting. */
  function setEffort(role: "chat" | "skill", value: string) {
    if (!config) return;
    const level = isEffortLevel(value) ? value : null;
    if (role === "chat") config.agents.chat_effort = level;
    else config.agents.skill_effort = level;
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
      claudeLoginError = messageOf(e);
    }
  }

  async function signIn() {
    signingIn = true;
    signInCode = "";
    try {
      claudeLogin = await invokeBackend<ClaudeLoginStatus>("claude_sign_in");
      claudeLoginError = null;
      toast(claudeLogin.logged_in ? tSettings("signedIn") : tSettings("signInIncomplete"), "info");
    } catch (e) {
      toast(tSettings("signInFailed", { error: messageOf(e) }), "warning");
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
      toast(tSettings("codeNotTaken", { error: messageOf(e) }), "warning");
    }
  }

  async function signOut() {
    try {
      claudeLogin = await invokeBackend<ClaudeLoginStatus>("claude_sign_out");
      toast(tSettings("signedOut"), "info");
    } catch (e) {
      toast(tSettings("signOutFailed", { error: messageOf(e) }), "warning");
    }
  }

  /** `save_config` returns `true` when only a restart will pick up the new
   *  workspace root (everything else applies live). */
  async function persist(): Promise<boolean> {
    if (!config) return false;
    const restart = await invokeBackend<boolean>("save_config", { newConfig: buildUpdate(config) });
    emit(CONFIG_SAVED);
    return restart;
  }

  async function saveVault() {
    if (!config?.workspace_root.trim()) {
      toast(tSettings("vaultEmpty"), "warning");
      return;
    }
    savingVault = true;
    try {
      const restartNeeded = await persist();
      toast(restartNeeded ? tSettings("vaultSavedRestart") : tSettings("vaultSaved"), "info");
    } catch (e) {
      toast(tSettings("savingFailed", { error: messageOf(e) }), "warning");
    } finally {
      savingVault = false;
    }
  }

  /** One role's provider: its role-relevant model field must be set + sane,
   *  its base URL https (or loopback), and its key present when required.
   *  Mirrors the per-role loop in Rust's `Config::validate_for_save`. */
  function roleProviderLooksSane(
    roleLabel: string,
    id: ProviderId,
    modelLabel: "chat_model" | "skill_model",
  ): string | null {
    if (!config) return null;
    const meta = metaFor(id);
    if (id === "anthropic") return null; // blank models = free CLI default
    const s = config.agents.providers[id];
    if (!s) return tSettings("noProviderSettings", { role: roleLabel, id });
    const hasKey = s.has_key || keyUpdateFor(id).kind === "set";
    const named = { role: roleLabel, provider: meta.label };
    if (meta.key === "required" && !hasKey) return tSettings("needsKey", named);

    const v = (modelLabel === "chat_model" ? s.chat_model : s.skill_model).trim();
    const field = modelLabel === "chat_model" ? tSettings("fieldChatModel") : tSettings("fieldSkillsModel");
    if (!v) return tSettings("modelEmpty", { ...named, field });
    if (/\s/.test(v)) return tSettings("modelSpaces", { ...named, field });
    const opens = (v.match(/\[/g) ?? []).length;
    const closes = (v.match(/\]/g) ?? []).length;
    if (opens !== closes || /[()]/.test(v)) {
      return tSettings("modelBrackets", { ...named, field, value: v });
    }
    const url = s.base_url?.trim();
    if (!url || !/^https:\/\/.+/.test(url)) {
      const loopback = /^http:\/\/(localhost|127\.0\.0\.1|\[::1\])(:\d+)?(\/|$)/.test(url ?? "");
      if (!loopback) return tSettings("baseUrlHttps", named);
    }
    return null;
  }

  /** Fast-path affordance only — `Config::validate_for_save` on the Rust side
   *  is the authority. Checks the chat provider's `chat_model` and the skill
   *  provider's `skill_model`; the 2026-09-08 incident was a `]`→`)` typo. */
  function providerLooksSane(): string | null {
    if (!config) return null;
    return (
      roleProviderLooksSane(tSettings("roleChat"), config.agents.chat_provider, "chat_model") ??
      roleProviderLooksSane(tSettings("roleSkills"), config.agents.skill_provider, "skill_model")
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
      toast(tSettings("providersSaved"), "info");
      keyEdits = {};
      webKeyEdits = {};
      await loadConfig();
      await refreshSpend();
    } catch (e) {
      toast(tSettings("savingFailed", { error: messageOf(e) }), "warning");
    } finally {
      savingProvider = false;
    }
  }

  /** The provider whose model list is being fetched again, if any. */
  let refreshingModels = $state<ProviderId | null>(null);

  /** The line under a provider's model fields: where its list comes from and how fresh it is. */
  function catalogStatus(id: ProviderId): string {
    if (id === "anthropic") return tSettings("catalogBuiltIn");
    const entry = $modelCatalog?.providers[id];
    if (!entry) return tSettings("catalogNone");
    if (entry.error) return tSettings("catalogFailed", { error: entry.error });
    const when = entry.fetched_at ? formatDateTime(entry.fetched_at) : tFormat("never");
    return tSettings("catalogFetched", { n: entry.models.length, when });
  }

  async function refreshModels(id: ProviderId) {
    refreshingModels = id;
    try {
      const outcome = await refreshModelCatalog(id);
      if (outcome.price_error) toast(tSettings("pricesNotSaved", { error: outcome.price_error }), "danger");
      else if (outcome.repriced.length > 0) {
        toast(tSettings("pricesUpdated", { models: outcome.repriced.join(", ") }), "info");
      }
    } catch (err) {
      toast(tSettings("modelListFailed", { error: messageOf(err) }), "danger");
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
    title={tSettings("title")}
    onClose={() => (open = false)}
    style="width: min(calc(620px * var(--ax-ui-scale)), calc(100vw - 2 * var(--ax-space-5)));
      max-height: calc(100vh - 2 * var(--ax-space-5));"
  >
    <nav class="tabs">
      {#each TABS as t (t.id)}
        <button type="button" class="tab" class:active={tab === t.id} onclick={() => (tab = t.id)}>
          {t.label()}
          {#if t.id === "workspace"}
            <HexCount count={$tagProposals.length} label={waitingLabel($tagProposals.length)} />
          {/if}
        </button>
      {/each}
    </nav>

    <div class="body">
      {#if tab === "appearance"}
        <section>
          <h3>{tSettings("theme")}</h3>
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
                    <span class="meta">{t.blurb()}</span>
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        </section>

        <section>
          <h3>{tSettings("typeface")}</h3>
          <div class="fonts" role="radiogroup" aria-label={tSettings("typeface")}>
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
                <span class="sample">{tSettings("fontSample")}</span>
                <span class="name">{f.label}</span>
              </button>
            {/each}
          </div>
          <p class="hint">{tSettings("typefaceHint")}</p>
        </section>

        <LanguageSetting />

        <section>
          <h3>{tSettings("motion")}</h3>
          <label class="row">
            <span class="label">{tSettings("movement")}</span>
            <select
              value={$motionSetting}
              onchange={(e) => {
                const v = e.currentTarget.value;
                if (isMotionLevel(v)) motionSetting.set(v);
              }}
            >
              {#each MOTION_LEVELS as level (level.id)}
                <option value={level.id}>{level.label()}</option>
              {/each}
            </select>
          </label>
          <p class="hint">
            {tSettings("motionHint")}
            {#if $systemReducesMotion}<strong>{tSettings("reduceMotionOn")}</strong>{/if}
          </p>
        </section>

        <section>
          <h3>{tSettings("light")}</h3>
          <label class="opt">
            <input type="checkbox" bind:checked={$effects.pointerLight} /> {tSettings("pointerLight")}
            <span class="hint">{tSettings("pointerLightHint")}</span>
          </label>
          <label class="opt">
            <input type="checkbox" bind:checked={$effects.dayLight} /> {tSettings("dayLight")}
            <span class="hint">{tSettings("dayLightHint")}</span>
          </label>
          <label class="opt">
            <input type="checkbox" bind:checked={$effects.ignition} /> {tSettings("ignition")}
            <span class="hint">{tSettings("ignitionHint")}</span>
          </label>
        </section>

        <section>
          <h3>{tSettings("sound")}</h3>
          <label class="opt">
            <input
              type="checkbox"
              bind:checked={$effects.sounds}
              onchange={() => {
                if ($effects.sounds) void unlockAudio();
              }}
            />
            {tSettings("sounds")}
            <span class="hint">{tSettings("soundsHint")}</span>
          </label>
          <div class="cues">
            {#each CUE_ORDER as cue (cue)}
              <button type="button" class="cue" onclick={() => void unlockAudio().then(() => playCue(cue))}>
                <Icon name="play" size="sm" />{CUE_LABELS[cue]()}
              </button>
            {/each}
          </div>
        </section>

        <section>
          <h3>{tSettings("canvas")}</h3>
          <label class="opt">
            <input type="checkbox" bind:checked={$showGrid} /> {tSettings("showGrid")}
            <span class="hint">{tSettings("showGridHint")}</span>
          </label>
          <label class="opt">
            <input type="checkbox" bind:checked={$snapEdges} /> {tSettings("magneticEdges")}
            <span class="hint">{tSettings("magneticEdgesHint")}</span>
          </label>
        </section>

        <section>
          <h3>{tSettings("orbitBrain")}</h3>
          <label class="opt">
            {tSettings("speed")}
            <input
              class="speed"
              type="range"
              min="0.005"
              max={SPEED_MAX}
              step="0.005"
              bind:value={$brainView.speed}
              disabled={!turns($motion)}
            />
          </label>
          <label class="opt">
            <input type="checkbox" bind:checked={$brainView.labels} /> {tSettings("labels")}
            <span class="hint">{tSettings("labelsHint")}</span>
          </label>
          <label class="opt">
            <input type="checkbox" bind:checked={$brainView.fileNames} /> {tSettings("fileNames")}
            <span class="hint">{tSettings("fileNamesHint")}</span>
          </label>
          <label class="row">
            <span class="label">{tSettings("shimmer")}</span>
            <select bind:value={$brainView.shine}>
              {#each SHINES as s (s.id)}
                <option value={s.id}>{s.label()}</option>
              {/each}
            </select>
            <span class="hint">{tSettings("shimmerHint")}</span>
          </label>
          <label class="row">
            <span class="label">{tSettings("layout")}</span>
            <select bind:value={$brainView.layout}>
              {#each BRAIN_LAYOUTS as l (l.id)}
                <option value={l.id}>{l.label()}</option>
              {/each}
            </select>
            <span class="hint">{tSettings("layoutHint")}</span>
          </label>
        </section>

        <section>
          <h3>{tSettings("uiSize")}</h3>
          <label class="opt">
            <input
              type="checkbox"
              checked={$uiSize === "auto"}
              onchange={(e) => uiSize.set(e.currentTarget.checked ? "auto" : autoPercent)}
            />
            {tSettings("auto")} <span class="hint">{tSettings("autoHint", { percent: autoPercent })}</span>
          </label>
          <label class="row">
            <span class="label">{tSettings("textAndControls")}</span>
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
          <p class="hint">{tSettings("uiSizeHint")}</p>
        </section>

        <section>
          <h3>{tSettings("keyboard")}</h3>
          <label class="row">
            <span class="label">{tSettings("shortcutsFollow")}</span>
            <select
              value={$keyboardSetting}
              onchange={(e) => {
                const v = e.currentTarget.value;
                if (isKeyboardSetting(v)) keyboardSetting.set(v);
              }}
            >
              {#each KEYBOARD_SETTINGS as option (option.value)}
                <option value={option.value}>
                  {option.label()}{option.value === "auto" ? ` (${platformLabel($platform)})` : ""}
                </option>
              {/each}
            </select>
          </label>
          <p class="hint">{tSettings("keyboardHint")}</p>
        </section>

        <section>
          <h3>{tSettings("glass")}</h3>
          <label class="row">
            <span class="label">{tSettings("transparency")}</span>
            <input
              type="range"
              min="0"
              max="100"
              step="5"
              disabled={$systemReducesTransparency}
              bind:value={$windowTransparency}
            />
            <span class="readout">{$windowTransparency}%</span>
          </label>
          <label class="row">
            <span class="label">{tSettings("blur")}</span>
            <input
              type="range"
              min="0"
              max={MAX_GROUND_BLUR}
              step="1"
              disabled={$systemReducesTransparency}
              bind:value={$glassBlur}
            />
            <span class="readout">{tSettings("pixels", { n: $glassBlur })}</span>
          </label>
          <div class="row">
            <span class="label">{tSettings("backgroundImage")}</span>
            <div class="actions">
              <button
                type="button"
                disabled={wallpaperBusy}
                onclick={() => void wallpaperStep(pickWallpaper, "imageNotTaken")}
              >
                {$wallpaper ? tSettings("chooseAnother") : tSettings("chooseImage")}
              </button>
              {#if $wallpaper}
                <button
                  type="button"
                  disabled={wallpaperBusy}
                  onclick={() => void wallpaperStep(clearWallpaper, "imageNotRemoved")}>{tSettings("remove")}</button
                >
              {/if}
            </div>
          </div>
          {#if $wallpaper}
            <label class="row">
              <span class="label">{tSettings("imageDim")}</span>
              <input type="range" min="0" max={MAX_GROUND_DIM} step="5" bind:value={$glassDim} />
              <span class="readout">{$glassDim}%</span>
            </label>
          {/if}
          <p class="hint">{tSettings("glassHint")}</p>
          {#if $systemReducesTransparency}
            <p class="hint">{tSettings("reduceTransparencyOn")}</p>
          {/if}
        </section>

        <section>
          <h3>{tSettings("customCss")}</h3>
          <p class="status {$customTheme.status}">{$customTheme.message() || tSettings("notLoaded")}</p>
          {#if $customTheme.errors.length > 0}
            <ul class="errors">
              {#each $customTheme.errors as e, i (i)}
                <li><code>{e.rule}</code>{#if e.property} · <code>{e.property}</code>{/if} — {e.message}</li>
              {/each}
            </ul>
          {/if}
          <div class="actions">
            <button type="button" disabled={reloading} onclick={reload}>
              {reloading ? tSettings("reloading") : tSettings("reloadCss")}
            </button>
            <button type="button" onclick={copyTemplate}>{tSettings("copyTemplate")}</button>
          </div>
          {#if showTemplate}
            <textarea readonly rows="8" spellcheck="false">{TEMPLATE}</textarea>
          {/if}
        </section>
      {:else if tab === "workspace"}
        {#if config}
          <section>
            <h3>{tSettings("vault")}</h3>
            <p class="lead">{tSettings("vaultLead")}</p>
            <label class="field">
              <span>{tSettings("path")}</span>
              <input
                type="text"
                spellcheck="false"
                bind:value={config.workspace_root}
                placeholder={tSettings("vaultPlaceholder")}
              />
            </label>
            <div class="actions">
              <button type="button" disabled={savingVault} onclick={saveVault}>
                {savingVault ? tSettings("saving") : tSettings("saveRestart")}
              </button>
            </div>
          </section>
        {/if}
        <TagsSettings />
      {:else if tab === "providers"}
        {#if config}
          <section>
            <h3>{tSettings("editProviders")}</h3>
            <p class="lead">{tSettings("editProvidersLead")}</p>
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
                        {#if config.agents.chat_provider === p.id}
                          <span class="role-badge">{tSettings("badgeChat")}</span>
                        {/if}
                        {#if config.agents.skill_provider === p.id}
                          <span class="role-badge">{tSettings("badgeSkills")}</span>
                        {/if}
                      </span>
                      <span class="meta">{p.blurb()}</span>
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
                  <p class="hint">{tSettings("anthropicNoKey")}</p>
                {:else}
                  <label class="field">
                    <span>{tSettings("baseUrl")}</span>
                    <input
                      type="text"
                      spellcheck="false"
                      bind:value={config.agents.providers[pid].base_url}
                      placeholder={"https://…"}
                    />
                  </label>
                  <label class="field">
                    <span>{editingMeta.key === "optional" ? tSettings("apiKeyOptional") : tSettings("apiKey")}</span>
                    <input
                      type="password"
                      autocomplete="off"
                      spellcheck="false"
                      value={keyEdits[pid] ?? ""}
                      oninput={(e) => setKeyEdit(pid, e.currentTarget.value)}
                      placeholder={config.agents.providers[pid].has_key
                        ? tSettings("keySaved")
                        : editingMeta.key === "optional"
                          ? tSettings("keyOllama")
                          : tSettings("keyRequired")}
                    />
                    {#if config.agents.providers[pid].has_key && (keyEdits[pid] ?? "") === "" && pid in keyEdits}
                      <span class="hint">{tSettings("keyDeletedOnSave")}</span>
                    {/if}
                  </label>
                {/if}
                <div class="field">
                  <span>{tSettings("chatModel")}</span>
                  <ModelPicker
                    label={tSettings("chatModel")}
                    bind:value={config.agents.providers[pid].chat_model}
                    options={providerOptions($modelCatalog, pid)}
                    placeholder={tSettings("modelExample", { model: "claude-sonnet-5" })}
                  />
                </div>
                <div class="field">
                  <span>{tSettings("skillsModel")}</span>
                  <ModelPicker
                    label={tSettings("skillsModel")}
                    bind:value={config.agents.providers[pid].skill_model}
                    options={providerOptions($modelCatalog, pid)}
                    placeholder={tSettings("modelExample", { model: "claude-haiku-5-5" })}
                    onRefresh={pid === "anthropic" ? undefined : () => void refreshModels(pid)}
                    refreshing={refreshingModels === pid}
                    status={catalogStatus(pid)}
                  />
                </div>
                <p class="hint">
                  {tSettings("modelOverrideBefore")} <code>{"model:"}</code> {tSettings("modelOverrideAfter")}
                </p>
              </div>
            {:else}
              <p class="status error">{tSettings("unknownProvider", { id: editingProvider })}</p>
            {/if}

            <div class="actions">
              <button type="button" disabled={savingProvider} onclick={saveProvider}>
                {savingProvider ? tSettings("saving") : tSettings("saveProvider")}
              </button>
            </div>
          </section>

          <section>
            <h3>{tSettings("harnessAssignment")}</h3>
            <p class="lead">
              {tSettings("harnessLeadBefore")} <code>{"backend:"}</code> {tSettings("harnessLeadAfter")}
            </p>
            <div class="role-picks">
              {#each ROLE_PICKS as r (r.role)}
                {@const harness = r.role === "chat" ? config.agents.chat_harness : config.agents.skill_harness}
                <div class="role-pick">
                  <label class="field">
                    <span>{tSettings("harnessFor", { role: r.label() })}</span>
                    <select
                      value={harness}
                      onchange={(e) => setHarness(r.role as "chat" | "skill", e.currentTarget.value as HarnessId)}
                    >
                      {#each HARNESSES as h (h.id)}<option value={h.id}>{h.label}</option>{/each}
                    </select>
                  </label>
                  <label class="field">
                    <span>{tSettings("providerFor", { role: r.label() })}</span>
                    {#if r.role === "chat"}
                      <select bind:value={config.agents.chat_provider}>
                        {#each providersFor(harness, PROVIDERS) as p (p.id)}
                          <option value={p.id}>{p.label}</option>
                        {/each}
                      </select>
                    {:else}
                      <select bind:value={config.agents.skill_provider}>
                        {#each providersFor(harness, PROVIDERS) as p (p.id)}
                          <option value={p.id}>{p.label}</option>
                        {/each}
                      </select>
                    {/if}
                  </label>
                  {#if harness === "axh"}
                    {@const role = r.role as "chat" | "skill"}
                    {@const provider = role === "chat" ? config.agents.chat_provider : config.agents.skill_provider}
                    {@const settings = config.agents.providers[provider]}
                    {@const model = (role === "chat" ? settings?.chat_model : settings?.skill_model) ?? ""}
                    {@const reasoning = reasoningOf($modelCatalog, provider, model)}
                    {@const stored = role === "chat" ? config.agents.chat_effort : config.agents.skill_effort}
                    {@const note = unlistedNote(reasoning, stored)}
                    <label class="field">
                      <span>{tSettings("effortFor", { role: r.label() })}</span>
                      <select value={stored ?? ""} onchange={(e) => setEffort(role, e.currentTarget.value)}>
                        <option value="">
                          {reasoning?.default_effort
                            ? tSettings("glutOwnEffortElse", { level: reasoning.default_effort })
                            : tSettings("glutOwnEffort")}
                        </option>
                        {#each offeredLevels(reasoning, stored) as level (level)}
                          <option value={level}>
                            {takesLevel(reasoning, level) ? level : tSettings("effortNotListed", { level })}
                          </option>
                        {/each}
                      </select>
                    </label>
                    {#if note}<p class="hint">{note}.</p>{/if}
                  {/if}
                  <p class="hint">{HARNESSES.find((h) => h.id === harness)?.blurb()}</p>
                </div>
              {/each}
            </div>
            {#if config.agents.skill_harness === "axh"}
              <p class="hint">
                {tSettings("webSkillBefore")}<code>{"news-web"}</code>{tSettings("webSkillAfter")}
              </p>
            {/if}

            {#if needsClaude}
              <div class="provider-form claude-login">
                <span class="title">{tSettings("claudeSignIn")}</span>
                {#if claudeLogin?.logged_in}
                  <p class="status applied">
                    {tSettings("signedInAs", { email: claudeLogin.email ?? tSettings("unknownEmail") })}
                    {#if claudeLogin.subscription}({claudeLogin.subscription}){/if}
                    {tSettings("ownLogin")} <code>{"~/.claude"}</code>.
                  </p>
                  <div class="actions">
                    <button type="button" onclick={signOut}>{tSettings("signOut")}</button>
                  </div>
                {:else}
                  <p class="status error">
                    {#if claudeLoginError}
                      {tSettings("claudeUnreachable", { error: claudeLoginError })}
                    {:else}
                      {tSettings("notSignedIn")}
                    {/if}
                  </p>
                  <div class="actions">
                    <button type="button" disabled={signingIn} onclick={signIn}>
                      {signingIn ? tSettings("waitingForBrowser") : tSettings("signInAnthropic")}
                    </button>
                  </div>
                  {#if signingIn}
                    <label class="field">
                      <span>{tSettings("signInCode")}</span>
                      <span class="code-row">
                        <input type="text" spellcheck="false" autocomplete="off" bind:value={signInCode} />
                        <button type="button" disabled={!signInCode.trim()} onclick={sendSignInCode}>
                          {tSettings("send")}
                        </button>
                      </span>
                    </label>
                  {/if}
                {/if}
              </div>
            {/if}

            <div class="provider-form web-search">
              <span class="title">{tSettings("webSearch")}</span>
              <p class="hint">
                {tSettings("webSearchBefore")} <code>{"web_search"}</code> {tSettings("webSearchMiddle")}
                <code>{"config.toml"}</code> {tSettings("webSearchAfter")}
              </p>
              {#each WEB_KEYS as k (k.id)}
                {@const stored = k.stored(config.agents.web)}
                <label class="field">
                  <span>{tSettings("webKey", { name: k.label })}</span>
                  <input
                    type="password"
                    autocomplete="off"
                    spellcheck="false"
                    value={webKeyEdits[k.id] ?? ""}
                    oninput={(e) => (webKeyEdits = { ...webKeyEdits, [k.id]: e.currentTarget.value })}
                    placeholder={stored
                      ? tSettings("keySaved")
                      : k.id === "exa"
                        ? tSettings("optional")
                        : tSettings("notSet")}
                  />
                  <span class="hint">{k.hint()}</span>
                  {#if stored && (webKeyEdits[k.id] ?? "") === "" && k.id in webKeyEdits}
                    <span class="hint">{tSettings("keyDeletedOnSave")}</span>
                  {/if}
                </label>
              {/each}
              <div class="role-picks">
                <label class="field">
                  <span>{tSettings("searchesPerDay")}</span>
                  <input type="number" min="0" step="1" bind:value={config.agents.web.daily_search_limit} />
                </label>
                <label class="field">
                  <span>{tSettings("tavilyPerMonth")}</span>
                  <input type="number" min="0" step="1" bind:value={config.agents.web.tavily_monthly_limit} />
                </label>
              </div>
              <p class="hint">{tSettings("webLimitsHint", { n: formatNumber(1000) })}</p>
            </div>

            <div class="provider-form spend-form">
              {#each spend as s (s.role)}
                {#if s.metered}
                  <p class="hint">
                    {tSettings("spend")}
                    <strong>{tSettings("roleToProvider", { role: s.role, provider: s.provider })}</strong>:
                    <strong>{tSettings("spendToday", { amount: usd(s.today_usd, 4) })}</strong>
                    {#if s.daily_cap_usd != null}{tSettings("spendCap", { amount: usd(s.daily_cap_usd, 2) })}{/if}
                    {tSettings("spendMonth", { amount: usd(s.month_usd, 2) })}
                  </p>
                {:else}
                  <p class="hint">{tSettings("subscriptionBilled", { role: s.role, provider: s.provider })}</p>
                {/if}
              {/each}
              <label class="field">
                <span>{tSettings("dailyLimit")}</span>
                <input
                  type="number"
                  min="0"
                  step="0.5"
                  placeholder={tSettings("noLimit")}
                  value={config.agents.daily_usd_cap ?? ""}
                  oninput={(e) => setDailyCap(e.currentTarget.value)}
                />
              </label>
              <p class="hint">{tSettings("dailyLimitHint")}</p>
            </div>

            <div class="actions">
              <button type="button" disabled={savingProvider} onclick={saveProvider}>
                {savingProvider ? tSettings("saving") : tSettings("saveAssignment")}
              </button>
            </div>
          </section>
        {/if}
      {:else if tab === "storage"}
        <StorageSettings />
      {:else if tab === "about"}
        <section>
          <h3>{tSettings("tabAbout")}</h3>
          {#if info}
            <dl>
              <dt>{tSettings("owner")}</dt><dd>{info.owner || tSettings("ownerUnset")}</dd>
              <dt>{tSettings("workspace")}</dt><dd>{info.workspace_root}</dd>
              <dt>{tSettings("version")}</dt><dd>{info.version}</dd>
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

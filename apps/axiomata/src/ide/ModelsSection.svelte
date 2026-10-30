<!--
  The Studio's models (`docs/plans/agents-model.md`, AM3, Q4, Q12, Q13, Q20): which models of the catalogs are switched
  on, which tier each one has and where that comes from, the default engine of each tier and the price limits of the
  price proposal. An automatic engine exists for every model that is on; the owner's own engines sit at the bottom
  (`EnginesSection`). Rust decides every rule (`model_tiers`, `engines`); this only shows and sends.

  Accent rule: orange only where something lives — the research run under way. A waiting proposal carries a warn mark
  and a plain "Accept"; the owner's chosen tier is a selection (`--ax-selected-bg`), not a glow.

  Every control shows the backend's state: a switch or a select that is changed is put back at once and redrawn from
  the answer's reload, so a refused change never stays on screen.
-->
<script lang="ts">
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";

  import { listenBackend } from "../core/backend";
  import { messageOf } from "../core/errors";
  import { TIERS, listEngines, type EngineEntry, type Tier } from "../core/roster";
  import { toast } from "../core/toast";
  import { uiScale } from "../core/uiScale";
  import Notice from "../ui/Notice.svelte";
  import SectionHead from "../ui/SectionHead.svelte";
  import StatusMark from "../ui/StatusMark.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import EnginesSection from "./EnginesSection.svelte";
  import {
    Latest,
    MODEL_TIERS_UPDATED,
    SMALL_LIST_MAX,
    choiceWithTier,
    contextLabel,
    engineOptionsFor,
    filterModels,
    isWebAddress,
    limitsAfterReload,
    limitsProblem,
    listModels,
    modelResearchRunning,
    modelsByProvider,
    priceLabel,
    proposalWaits,
    providerHeading,
    researchModelTiers,
    researchToast,
    setDefaultEngine,
    setModelChoice,
    setTierLimits,
    sourceHost,
    tierOrigin,
    toggledChoice,
    withBusy,
    withoutBusy,
    type ModelEntry,
    type ModelsOverview,
  } from "./models";
  import { refreshEngines } from "./rosterStore";

  /** The catalog was refreshed (`model_watch.rs`): the lists may have changed. */
  const CATALOG_UPDATED = "models:catalog-updated";
  /** Below this width (unscaled px) a model's tier goes under its name; times the UI scale, like every size. */
  const NARROW_PX = 640;

  let overview = $state<ModelsOverview | null>(null);
  let engines = $state<EngineEntry[]>([]);
  let loading = $state(true);
  let running = $state(false);
  /** The filter text of each provider's list. */
  let queries = $state<Record<string, string>>({});
  /** The price limits as typed, before they are saved; `editingLimits` while the owner types them. */
  let lightBelow = $state("");
  let heavyAbove = $state("");
  let editingLimits = $state(false);
  /** The rows whose change is on its way: each waits on its own answer. */
  let busy = $state<Set<string>>(new Set());
  /** Only the newest reload's answer is shown. */
  const latest = new Latest();
  let width = $state(0);
  const narrow = $derived(width > 0 && width < NARROW_PX * $uiScale);

  const groups = $derived(overview ? modelsByProvider(overview.models) : []);
  const limitsTyped = $derived({ light: Number(lightBelow), heavy: Number(heavyAbove) });
  const limitsChanged = $derived(
    overview !== null &&
      (limitsTyped.light !== overview.tier_limits.light_below || limitsTyped.heavy !== overview.tier_limits.heavy_above),
  );
  const limitsError = $derived(limitsProblem(limitsTyped.light, limitsTyped.heavy));

  async function reload() {
    const ticket = latest.next();
    try {
      const [models, catalog, researching] = await Promise.all([listModels(), listEngines(), modelResearchRunning()]);
      if (!latest.current(ticket)) return;
      overview = models;
      engines = catalog;
      running = researching;
      ({ lightBelow, heavyAbove } = limitsAfterReload({ lightBelow, heavyAbove }, models.tier_limits, editingLimits));
    } catch (err) {
      if (latest.current(ticket)) toast(`Could not load the models: ${messageOf(err)}`, "warning");
    } finally {
      if (latest.current(ticket)) loading = false;
    }
  }

  /**
   * Runs a change, then reloads the list and the engine pickers elsewhere (the engines follow the models). The reload
   * runs on a refusal too, so what is shown is what the backend holds.
   */
  async function change(key: string, act: () => Promise<unknown>): Promise<boolean> {
    busy = withBusy(busy, key);
    let done = false;
    try {
      await act();
      done = true;
      void refreshEngines();
    } catch (err) {
      toast(messageOf(err), "warning");
    } finally {
      busy = withoutBusy(busy, key);
      await reload();
    }
    return done;
  }

  const toggle = (model: ModelEntry) => change(model.key, () => setModelChoice(model.key, toggledChoice(model)));
  const rate = (model: ModelEntry, tier: Tier | null) =>
    change(model.key, () => setModelChoice(model.key, choiceWithTier(model, tier)));
  const accept = (model: ModelEntry) => (model.proposal ? rate(model, model.proposal.tier) : undefined);

  async function research(keys: string[] | null) {
    // Shown at once; the backend's flag decides from here on (a fast run may end before the answer arrives).
    running = true;
    try {
      const asked = await researchModelTiers(keys);
      toast(researchToast(asked.length), "info");
    } catch (err) {
      toast(messageOf(err), "warning");
    }
    running = await modelResearchRunning().catch(() => false);
  }

  const setDefault = (tier: Tier, id: string) => change(`default:${tier}`, () => setDefaultEngine(tier, id || null));

  async function saveLimits() {
    if (limitsError) return;
    const limits = { light_below: limitsTyped.light, heavy_above: limitsTyped.heavy };
    // Saved: the form takes the stored limits again; refused: the owner's typing stays to be corrected.
    const saved = await change("limits", async () => {
      await setTierLimits(limits);
      editingLimits = false;
    });
    if (saved) ({ lightBelow, heavyAbove } = limitsAfterReload({ lightBelow, heavyAbove }, limits, false));
  }

  /** Opens a proposal's source in the system browser; in the browser mock, in a new tab. */
  function openSource(source: string) {
    if (!isWebAddress(source)) return;
    void openUrl(source).catch(() => window.open(source, "_blank", "noopener,noreferrer"));
  }

  /** An engine as a select shows it: "Claude · Haiku 5.5", "Glut · GLM 5.3" — its label for an own one. */
  const engineName = (engine: EngineEntry | undefined): string =>
    !engine
      ? ""
      : engine.origin === "auto" && engine.model_name
        ? `${engine.harness === "claude_code" ? "Claude" : "Glut"} · ${engine.model_name}`
        : engine.label;

  const tierLabel = (tier: Tier | null) => TIERS.find((t) => t.id === tier)?.label ?? "—";

  onMount(() => {
    let destroyed = false;
    const stops: (() => void)[] = [];
    /** A listener that arrives after the pane is gone is stopped at once, never left behind. */
    const keep = (stop: () => void) => (destroyed ? stop() : stops.push(stop));
    void reload();
    // `reload` asks the backend whether research still runs: the event's word alone could be stale.
    void listenBackend(MODEL_TIERS_UPDATED, () => void reload()).then(keep);
    void listenBackend(CATALOG_UPDATED, () => void reload()).then(keep);
    return () => {
      destroyed = true;
      stops.forEach((stop) => stop());
    };
  });
</script>

<div class="models" class:narrow bind:clientWidth={width}>
  <div class="column">
    {#if running}
      <div class="slot">
        <Notice tone="live" title="Researching model tiers…">
          {#snippet details()}
            <p>
              The <code>model-tiering</code> skill looks the models up on rating sites and proposes a tier for each. Its
              proposals appear here as “not reviewed”; your own tier always wins.
            </p>
          {/snippet}
        </Notice>
      </div>
    {/if}

    {#if loading}
      <p class="hint">Loading…</p>
    {:else if overview}
      <section class="block">
        <SectionHead label="Default engine per tier" />
        <p class="hint">
          What a session of a tier starts on. Without a choice the cheapest engine of the tier runs; when one cannot
          run, the next of the same tier takes over.
        </p>
        <div class="defaults">
          {#each TIERS as t (t.id)}
            {@const chosen = overview.default_engines[t.id] ?? ""}
            {@const options = engineOptionsFor(engines, t.id)}
            <label class="default">
              <span class="tier-name">{t.label}</span>
              <select
                value={chosen}
                disabled={busy.has(`default:${t.id}`)}
                onchange={(e) => {
                  const picked = e.currentTarget.value;
                  e.currentTarget.value = chosen;
                  void setDefault(t.id, picked);
                }}
              >
                <option value="">Cheapest{options[0] ? `: ${engineName(options[0])}` : " of the tier"}</option>
                {#if chosen && !options.some((o) => o.id === chosen)}
                  <option value={chosen}>{engineName(engines.find((e) => e.id === chosen)) || chosen} — not of this tier</option>
                {/if}
                {#each options as o (o.id)}
                  <option value={o.id}>{engineName(o)}</option>
                {/each}
              </select>
            </label>
          {/each}
        </div>
      </section>

      <section class="block">
        <SectionHead label="Price limits" />
        <p class="hint">The tier a model's output price proposes until research or you say otherwise (USD per million output tokens).</p>
        <div class="limits">
          <label class="limit">
            <span>Light below</span>
            <input type="number" min="0" step="0.1" bind:value={lightBelow} oninput={() => (editingLimits = true)} />
          </label>
          <label class="limit">
            <span>Heavy above</span>
            <input type="number" min="0" step="0.5" bind:value={heavyAbove} oninput={() => (editingLimits = true)} />
          </label>
          <button
            type="button"
            class="ax-btn"
            disabled={!limitsChanged || limitsError !== null || busy.has("limits")}
            onclick={() => void saveLimits()}>Save limits</button
          >
        </div>
        {#if limitsChanged && limitsError}<p class="problem">{limitsError}</p>{/if}
      </section>

      <section class="block">
        <SectionHead label="Models">
          {#snippet actions()}
            <button type="button" class="ax-btn" disabled={running} onclick={() => void research(null)}>
              Propose all again
            </button>
          {/snippet}
        </SectionHead>
        <p class="hint">
          Each model that is on is an engine of its harness. Its tier comes from you, else from research, else from its
          price; a model without any is no agent yet.
        </p>
      </section>

      {#each groups as group (group.provider)}
        {@const query = queries[group.provider] ?? ""}
        {@const shown = filterModels(group.models, query)}
        {@const on = group.models.filter((m) => m.enabled).length}
        <section class="block">
          <SectionHead label={providerHeading(group.provider)} count={`${on} of ${group.models.length} on`} level={4} />
          {#if group.models.length > SMALL_LIST_MAX}
            <input
              class="filter"
              type="search"
              placeholder="Filter {group.models.length} models…"
              value={query}
              oninput={(e) => (queries = { ...queries, [group.provider]: e.currentTarget.value })}
            />
          {/if}
          <ul class="list">
            {#each shown as m (m.key)}
              <li class="model" class:off={!m.enabled}>
                <label class="switch" title={m.enabled ? "Switch off" : "Switch on"}>
                  <input
                    type="checkbox"
                    role="switch"
                    checked={m.enabled}
                    disabled={busy.has(m.key)}
                    aria-label="{m.short_name} on"
                    onchange={(e) => {
                      e.currentTarget.checked = m.enabled;
                      void toggle(m);
                    }}
                  />
                  <span class="track" aria-hidden="true"></span>
                </label>
                <div class="who">
                  <span class="name">{m.short_name}</span>
                  <span class="meta">
                    <code>{m.key}</code>
                    <span>{priceLabel(m)}</span>
                    {#if contextLabel(m.context_length)}<span>{contextLabel(m.context_length)}</span>{/if}
                  </span>
                </div>
                <div class="rating">
                  <div class="segments" role="radiogroup" aria-label="Tier of {m.short_name}">
                    {#each TIERS as t (t.id)}
                      {@const mine = m.owner_tier === t.id}
                      {@const applies = m.tier === t.id}
                      <button
                        type="button"
                        role="radio"
                        aria-checked={mine}
                        class:mine
                        class:applies={applies && !mine}
                        disabled={busy.has(m.key)}
                        title={mine ? "Your tier — click to drop it" : `Set ${t.label.toLowerCase()} as your tier`}
                        onclick={() => void rate(m, mine ? null : t.id)}>{t.label}</button
                      >
                    {/each}
                  </div>
                  <span class="origin" class:unrated={m.tier_source === "unrated"}>
                    {#if proposalWaits(m)}<StatusMark tone="warn" label="Waits for your review" />{/if}
                    {tierOrigin(m)}{#if m.tier && m.tier_source !== "owner"}: {tierLabel(m.tier)}{/if}
                  </span>
                </div>
                <div class="row-actions">
                  <IconButton
                    icon="refresh-cw"
                    size="sm"
                    label="Propose again for {m.short_name}"
                    disabled={running || !m.enabled}
                    onclick={() => void research([m.key])}
                  />
                </div>
                {#if m.proposal && m.tier_source !== "owner"}
                  <div class="proposal">
                    <span class="reason">{m.proposal.reason}</span>
                    {#if isWebAddress(m.proposal.source)}
                      <button type="button" class="source" onclick={() => openSource(m.proposal?.source ?? "")}>
                        {sourceHost(m.proposal.source)}
                      </button>
                    {:else}
                      <span class="source-text">{m.proposal.source}</span>
                    {/if}
                    {#if proposalWaits(m)}
                      <button type="button" class="ax-btn small" disabled={busy.has(m.key)} onclick={() => void accept(m)}>
                        Accept {tierLabel(m.proposal.tier)}
                      </button>
                    {/if}
                  </div>
                {/if}
              </li>
            {:else}
              <li class="empty">No model matches “{query}”.</li>
            {/each}
          </ul>
        </section>
      {/each}

      <section class="block own">
        <SectionHead label="Own engines" />
        <p class="hint">
          An engine of your own: a command or an environment the automatic ones do not have. Automatic engines are not
          edited here — switch their model on or off above.
        </p>
        <div class="own-editor">
          <EnginesSection onChange={() => void reload()} />
        </div>
      </section>
    {/if}
  </div>
</div>

<style>
  .models {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
  }

  .column {
    max-width: calc(980px * var(--ax-ui-scale));
    margin: 0 auto;
    padding: var(--ax-space-3) var(--ax-space-4) var(--ax-space-6);
  }

  .slot {
    margin-bottom: var(--ax-space-3);
  }

  .block {
    margin-bottom: var(--ax-space-5);
  }

  .hint {
    margin: var(--ax-space-1) 0 var(--ax-space-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
    line-height: var(--ax-line-height);
  }

  .slot code {
    font-family: var(--ax-font-mono);
    color: var(--ax-text);
  }

  .problem {
    margin: var(--ax-space-2) 0 0;
    color: var(--ax-warning);
    font-size: var(--ax-font-size-xs);
  }

  .defaults {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(calc(220px * var(--ax-ui-scale)), 1fr));
    gap: var(--ax-space-3);
  }

  .default,
  .limit {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    font-size: var(--ax-font-size-sm);
  }

  .tier-name,
  .limit > span {
    color: var(--ax-text-muted);
  }

  select,
  input[type="number"],
  .filter {
    padding: var(--ax-space-1) var(--ax-space-2);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text);
    background: var(--ax-bg);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }

  select:focus,
  input:focus {
    outline: none;
    border-color: var(--ax-focus-ring);
  }

  .limits {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--ax-space-3);
  }

  .limit input {
    width: calc(110px * var(--ax-ui-scale));
  }

  .filter {
    width: 100%;
    margin-bottom: var(--ax-space-2);
  }

  .list {
    list-style: none;
    margin: var(--ax-space-2) 0 0;
    padding: 0;
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-2);
  }

  .model {
    position: relative;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    align-items: center;
    gap: var(--ax-space-2) var(--ax-space-3);
    padding: var(--ax-space-2) var(--ax-space-3);
    border-top: 1px solid var(--ax-border);
  }

  .model:first-child {
    border-top: 0;
  }

  .model.off .who,
  .model.off .rating {
    opacity: 0.55;
  }

  .empty {
    padding: var(--ax-space-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }

  /* A switch drawn from tokens: the checkbox stays the control (keyboard, screen readers). */
  .switch {
    position: relative;
    display: inline-flex;
    cursor: pointer;
  }

  .switch input {
    position: absolute;
    inset: 0;
    opacity: 0;
    margin: 0;
    cursor: pointer;
  }

  .track {
    width: calc(30px * var(--ax-ui-scale));
    height: calc(17px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-3);
    border: 1px solid var(--ax-border-strong);
    position: relative;
    transition: background var(--ax-dur-fast) var(--ax-ease);
  }

  .track::after {
    content: "";
    position: absolute;
    top: calc(1px * var(--ax-ui-scale));
    left: calc(1px * var(--ax-ui-scale));
    width: calc(13px * var(--ax-ui-scale));
    height: calc(13px * var(--ax-ui-scale));
    border-radius: var(--ax-radius-pill);
    background: var(--ax-text-muted);
    transition: transform var(--ax-dur-fast) var(--ax-ease);
  }

  /* On is "fine", not "alive": the success tone, never the accent. */
  .switch input:checked + .track {
    background: color-mix(in srgb, var(--ax-success) 45%, var(--ax-surface-3));
    border-color: color-mix(in srgb, var(--ax-success) 60%, var(--ax-border-strong));
  }

  .switch input:checked + .track::after {
    transform: translateX(calc(13px * var(--ax-ui-scale)));
    background: var(--ax-text);
  }

  .switch input:focus-visible + .track {
    outline: 2px solid var(--ax-focus-ring);
    outline-offset: 2px;
  }

  .who {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: calc(2px * var(--ax-ui-scale));
  }

  .name {
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 0 var(--ax-space-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .meta code {
    font-family: var(--ax-font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }

  .rating {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: calc(2px * var(--ax-ui-scale));
  }

  .segments {
    display: inline-flex;
    padding: calc(2px * var(--ax-ui-scale));
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-1);
  }

  .segments button {
    padding: calc(1px * var(--ax-ui-scale)) var(--ax-space-2);
    border: 0;
    border-radius: var(--ax-radius-pill);
    background: none;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  .segments button:hover:not(:disabled) {
    color: var(--ax-text);
  }

  /* The owner's own value: a selection. What applies from a proposal or the price: only outlined. */
  .segments button.mine {
    background: var(--ax-selected-bg);
    color: var(--ax-text);
    font-weight: 600;
  }

  .segments button.applies {
    outline: 1px dashed var(--ax-border-strong);
    outline-offset: -1px;
    color: var(--ax-text);
  }

  .origin {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    white-space: nowrap;
  }


  .origin.unrated {
    font-style: italic;
  }

  .row-actions {
    display: flex;
  }

  .proposal {
    grid-column: 2 / -1;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--ax-space-1) var(--ax-space-3);
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
    line-height: var(--ax-line-height);
  }

  .reason {
    flex: 1 1 calc(260px * var(--ax-ui-scale));
  }

  .source {
    padding: 0;
    border: 0;
    background: none;
    color: var(--ax-syntax-link);
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
  }

  .source-text {
    font-style: italic;
  }

  .small {
    padding: calc(1px * var(--ax-ui-scale)) var(--ax-space-2);
    font-size: var(--ax-font-size-xs);
  }

  .own-editor {
    display: flex;
    height: calc(460px * var(--ax-ui-scale));
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    overflow: hidden;
  }

  /* In a narrow pane (`NARROW_PX` times the UI scale) the tier and its source go under the name. */
  .narrow .model {
    grid-template-columns: auto minmax(0, 1fr) auto;
  }

  .narrow .rating {
    grid-column: 2 / -1;
    align-items: flex-start;
  }
</style>

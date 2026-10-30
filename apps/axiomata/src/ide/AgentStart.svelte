<!--
  "Start an agent" (`docs/plans/agents-model.md`, AM4, Q16): a role, then light · medium · heavy (set to the role's
  default tier), then an engine of that tier with the tier's default marked, then Start — two clicks at the least
  (role, Start). The star keeps the current choice as a favorite (Q18). Rust resolves the engine and names the session
  (`agent_start::start`); the session is made the one way sessions are made.

  One component for the Agents tab in every mode of the Studio — Code, Canvas and Flow share the activity rail.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { IdeAgent } from "../core/backend";
  import { messageOf } from "../core/errors";
  import { TIERS, type Role, type Tier } from "../core/roster";
  import { toast } from "../core/toast";
  import HexAvatar from "../ui/HexAvatar.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import SectionHead from "../ui/SectionHead.svelte";
  import {
    engineChoiceLabel,
    enginesOf,
    isFavorite,
    listTierEngines,
    startAgentChoice,
    startableRoles,
    toggleFavorite,
    type Favorite,
    type TierEngines,
  } from "./agentStart";
  import { roleBadge, roleTitle, roleTone } from "./rosterLook";
  import { rosterVersion } from "./rosterStore";

  interface Props {
    roles: Role[];
    projectId: number;
    favorites: readonly Favorite[];
    /** The session was made; the caller opens its pane. */
    onStarted: (agent: IdeAgent) => void;
    /** A favorite was added or removed. */
    onFavorites: () => void;
  }

  let { roles, projectId, favorites, onStarted, onFavorites }: Props = $props();

  const offered = $derived(startableRoles(roles));
  let byTier = $state<TierEngines[]>([]);
  let roleName = $state<string | null>(null);
  let tier = $state<Tier>("medium");
  /** `null` = the tier's default engine, resolved at the start. */
  let engineId = $state<string | null>(null);
  let starting = $state(false);

  const role = $derived(offered.find((r) => r.name === roleName) ?? null);
  const engines = $derived(enginesOf(byTier, tier));
  const choice = $derived<Favorite | null>(role ? { role: role.name, tier, engine: engineId } : null);
  const favorite = $derived(choice !== null && isFavorite(favorites, choice));

  /** Counts reads, so only the newest answer is shown. */
  let enginesRead = 0;

  async function loadEngines() {
    const mine = ++enginesRead;
    try {
      const answer = await listTierEngines();
      if (mine === enginesRead) byTier = answer;
    } catch (err) {
      if (mine === enginesRead) toast(`Could not load the engines: ${messageOf(err)}`, "warning");
    }
  }

  /** A click opens the role's menu below the tiles (its default tier set); a second click on it closes the menu. */
  function pickRole(r: Role) {
    if (roleName === r.name) {
      roleName = null;
      return;
    }
    roleName = r.name;
    tier = r.tier;
    engineId = null;
  }

  function pickTier(t: Tier) {
    tier = t;
    engineId = null;
  }

  async function start() {
    if (!role || starting) return;
    starting = true;
    try {
      onStarted(await startAgentChoice(projectId, role.name, tier, engineId));
      roleName = null;
    } catch (err) {
      toast(messageOf(err), "warning");
    } finally {
      starting = false;
    }
  }

  async function star() {
    if (!choice) return;
    try {
      await toggleFavorite(choice);
      onFavorites();
    } catch (err) {
      toast(messageOf(err), "warning");
    }
  }

  onMount(() => void loadEngines());

  // The models or engines changed in the Models tab: the tiers' engines may have too.
  let first = true;
  $effect(() => {
    void $rosterVersion;
    if (first) {
      first = false;
      return;
    }
    void loadEngines();
  });
</script>

<section class="start" aria-label="Start an agent">
  <SectionHead label="Start an agent" />
  <div class="roles" role="radiogroup" aria-label="Role">
    {#each offered as r (r.name)}
      <button
        type="button"
        role="radio"
        class="role {roleTone(r.kind)}"
        class:picked={roleName === r.name}
        aria-checked={roleName === r.name}
        title={r.description || r.name}
        onclick={() => pickRole(r)}
      >
        <HexAvatar text={roleBadge(r.name)} ring="var(--tone)" />
        <span class="role-name">{roleTitle(r.name)}</span>
      </button>
    {/each}
  </div>

  {#if role}
    {#if role.description}<p class="hint">{role.description}</p>{/if}
    <div class="segments" role="radiogroup" aria-label="Tier">
      {#each TIERS as t (t.id)}
        <button
          type="button"
          role="radio"
          aria-checked={tier === t.id}
          class:picked={tier === t.id}
          title={t.hint}
          onclick={() => pickTier(t.id)}>{t.label}</button
        >
      {/each}
    </div>

    <ul class="engines" role="radiogroup" aria-label="Engine">
      <li>
        <button
          type="button"
          role="radio"
          aria-checked={engineId === null}
          class:picked={engineId === null}
          disabled={engines.length === 0}
          onclick={() => (engineId = null)}
        >
          <span class="engine-name">Default of the tier</span>
          <span class="engine-meta">
            {engines[0] ? engineChoiceLabel(engines[0]) : "no engine of this tier can run"}
          </span>
        </button>
      </li>
      {#each engines as e, at (e.id)}
        <li>
          <button
            type="button"
            role="radio"
            aria-checked={engineId === e.id}
            class:picked={engineId === e.id}
            onclick={() => (engineId = e.id)}
          >
            <span class="engine-name">{engineChoiceLabel(e)}</span>
            <span class="engine-meta">
              {#if at === 0}<span class="standard">standard</span>{/if}
              {e.output_per_m === null ? "" : e.output_per_m === 0 ? "local" : `$${e.output_per_m} / M out`}
              {e.billing === "subscription" ? "· subscription" : ""}
            </span>
          </button>
        </li>
      {/each}
    </ul>

    <div class="actions">
      <button
        type="button"
        class="ax-btn primary"
        disabled={starting || engines.length === 0}
        onclick={() => void start()}
      >
        {starting ? "Starting…" : "Start"}
      </button>
      <IconButton
        icon="star"
        size="sm"
        pressed={favorite}
        label={favorite ? "Remove this choice from the favorites" : "Keep this choice as a favorite"}
        onclick={() => void star()}
      />
    </div>
  {:else}
    <p class="hint">Pick a role, then its tier and model below.</p>
  {/if}
</section>

<style>
  .start {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    margin-bottom: var(--ax-space-3);
  }

  /* The roles as small tiles, hexagon on top like the masters' (Q16): as many to a row as the column takes. */
  .roles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(calc(96px * var(--ax-ui-scale)), 1fr));
    gap: var(--ax-space-2);
  }

  .role {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--ax-space-1);
    min-width: 0;
    padding: var(--ax-space-2) var(--ax-space-1);
    border: 1px solid color-mix(in srgb, var(--tone) 30%, var(--ax-border));
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-2);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    text-align: center;
    cursor: pointer;
  }

  .role:hover {
    border-color: color-mix(in srgb, var(--tone) 60%, var(--ax-border));
    background: var(--ax-surface-3);
  }

  /* Selected ≠ alive (accent rule): the picked role is a selection, not orange. */
  .role.picked {
    border-color: var(--ax-border-strong);
    background: var(--ax-selected-bg);
  }

  .role-name {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: var(--ax-font-size-sm);
    font-weight: 600;
  }

  /* The tone of the role's kind of work, as `RoleChip` tints it. */
  .plan {
    --tone: var(--ax-accent);
  }

  .review {
    --tone: var(--ax-success);
  }

  .implement {
    --tone: var(--ax-harness-opencode);
  }

  .other {
    --tone: var(--ax-text-muted);
  }

  .segments {
    display: inline-flex;
    align-self: flex-start;
    padding: calc(2px * var(--ax-ui-scale));
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-1);
  }

  .segments button {
    padding: calc(2px * var(--ax-ui-scale)) var(--ax-space-3);
    border: 0;
    border-radius: var(--ax-radius-pill);
    background: none;
    color: var(--ax-text-muted);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    cursor: pointer;
  }

  .segments button.picked {
    background: var(--ax-selected-bg);
    color: var(--ax-text);
    font-weight: 600;
  }

  /* A long model list (an OpenRouter account without its own allow-list) scrolls instead of pushing the page away:
     about five engines show at once. */
  .engines {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    max-height: calc(240px * var(--ax-ui-scale));
    overflow-y: auto;
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-2);
  }

  .engines li {
    flex: 0 0 auto;
  }

  .engines li + li {
    border-top: 1px solid var(--ax-border);
  }

  .engines button {
    display: flex;
    flex-direction: column;
    gap: calc(1px * var(--ax-ui-scale));
    width: 100%;
    padding: var(--ax-space-1) var(--ax-space-2);
    border: 0;
    background: none;
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    text-align: left;
    cursor: pointer;
  }

  .engines button:hover:not(:disabled) {
    background: var(--ax-surface-3);
  }

  .engines button.picked {
    background: var(--ax-selected-bg);
  }

  .engines button:disabled {
    cursor: default;
    opacity: 0.6;
  }

  .engine-name {
    font-size: var(--ax-font-size-sm);
  }

  .engine-meta {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  .standard {
    margin-right: var(--ax-space-1);
    padding: 0 var(--ax-space-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-sm);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
  }

  .hint {
    margin: 0;
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    line-height: var(--ax-line-height);
  }
</style>

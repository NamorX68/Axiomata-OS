<!--
  The Skills Deck's flip side: the deck's filter and order, and "Bundled skills" — every skill that ships with the
  app, its installed copy's state, and the update that brings it to the bundled version (`skills::bundled`). The seed
  never overwrites, so this is where a newer bundled skill reaches an install; a copy that may hold the owner's edits
  is only replaced after the question under its row.
-->
<script lang="ts">
  import { messageOf } from "../core/errors";
  import { onMount, tick } from "svelte";

  import type { BundledSkill } from "../core/backend";
  import { BACKEND_CHOICES } from "../core/harness";
  import type { ModuleContext } from "../core/types";
  import Notice from "../ui/Notice.svelte";
  import SectionHead from "../ui/SectionHead.svelte";
  import StatusMark from "../ui/StatusMark.svelte";
  import { tCanvas } from "../i18n/canvas";
  import { bundledFocus, bundledSkills, isOwners, stateLabel, stateTone, updatableCount } from "./bundledSkills";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  function set(key: string, value: string) {
    config.update((c) => ({ ...c, [key]: value }));
  }
  const value = (e: Event) => (e.currentTarget as HTMLSelectElement).value;

  let section = $state<HTMLElement | null>(null);
  let busy = $state(false);
  let error = $state("");
  /** The skill whose replacement waits for the owner's answer. */
  let asking = $state<string | null>(null);
  const updatable = $derived($bundledSkills ? updatableCount($bundledSkills) : 0);

  async function call(cmd: string, args?: Record<string, unknown>) {
    busy = true;
    try {
      bundledSkills.set(await ctx.invoke<BundledSkill[]>(cmd, args));
      error = "";
    } catch (err) {
      error = messageOf(err);
    } finally {
      busy = false;
    }
  }

  function update(skill: BundledSkill) {
    if (isOwners(skill.state)) {
      asking = skill.name;
      return;
    }
    void call("bundled_skill_update", { name: skill.name, replaceOwnEdit: false });
  }

  function replace(name: string) {
    asking = null;
    void call("bundled_skill_update", { name, replaceOwnEdit: true });
  }

  onMount(() => void call("bundled_skills_status"));

  // The front's notice asks for this section: scroll it into view on every ask, the first one included.
  $effect(() => {
    if ($bundledFocus > 0 && section) void tick().then(() => section?.scrollIntoView({ block: "start" }));
  });
</script>

<div class="settings">
  <label>
    {tCanvas("backend")}
    <select value={typeof $config.filterBackend === "string" ? $config.filterBackend : ""} onchange={(e) => set("filterBackend", value(e))}>
      <option value="">{tCanvas("allBackends")}</option>
      {#each BACKEND_CHOICES as c (c.id)}<option value={c.id}>{c.label}</option>{/each}
    </select>
  </label>
  <label>
    {tCanvas("order")}
    <select value={$config.order === "recent" ? "recent" : "name"} onchange={(e) => set("order", value(e))}>
      <option value="name">{tCanvas("byName")}</option>
      <option value="recent">{tCanvas("recentFirst")}</option>
    </select>
  </label>

  <section class="bundled" bind:this={section} aria-label={tCanvas("bundledSkills")}>
    <SectionHead label={tCanvas("bundledSkills")} rule>
      {#snippet actions()}
        <button
          type="button"
          class="ax-btn"
          disabled={busy || updatable === 0}
          title={tCanvas("updateAllTitle")}
          onclick={() => void call("bundled_skills_update_outdated")}>{tCanvas("updateAll")}</button
        >
      {/snippet}
    </SectionHead>
    {#if error}<p class="error">{error}</p>{/if}
    {#if $bundledSkills === null}
      <p class="muted">{tCanvas("readingBundled")}</p>
    {:else}
      <ul>
        {#each $bundledSkills as skill (skill.name)}
          <li>
            <span class="row">
              <span class="name">/{skill.name}</span>
              <span class="state">
                <StatusMark tone={stateTone(skill.state)} />
                {stateLabel(skill.state)}
              </span>
              <button
                type="button"
                class="ax-btn"
                disabled={busy || skill.state === "up_to_date"}
                aria-label={tCanvas("updateName", { name: skill.name })}
                onclick={() => update(skill)}>{tCanvas("update")}</button
              >
            </span>
            {#if asking === skill.name}
              <Notice title={tCanvas("replaceNotice", { name: skill.name })} alert>
                {#snippet action()}
                  <button type="button" class="ax-btn danger" disabled={busy} onclick={() => replace(skill.name)}>
                    {tCanvas("replace")}
                  </button>
                  <button type="button" class="ax-btn" onclick={() => (asking = null)}>{tCanvas("keepMine")}</button>
                {/snippet}
              </Notice>
            {/if}
          </li>
        {/each}
      </ul>
      <p class="muted">{tCanvas("onlyBundled")}</p>
    {/if}
  </section>
</div>

<style>
  .settings {
    padding: var(--ax-space-3);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }
  label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--ax-space-3);
  }

  .bundled {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    margin-top: var(--ax-space-3);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }
  li {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: center;
    gap: var(--ax-space-3);
  }
  .name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
    font: var(--ax-type-caption);
    white-space: nowrap;
  }
  p {
    margin: 0;
  }
  .muted {
    color: var(--ax-text-muted);
    font: var(--ax-type-caption);
  }
  .error {
    color: var(--ax-danger);
  }
</style>

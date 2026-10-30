<!--
  A project can bring its own agent roles (`.axiomata/agents/<name>/AGENT.md`, a2a.md CP-A1). Their instructions are
  text an agent obeys, so — like a project's `tasks.json` — they stay off until the owner has read them and said
  yes, for exactly this content (the hash shown here is what the confirmation is for; Rust refuses it if the files
  changed meanwhile). A role never names a command line, only engines of the owner's catalog.

  The review shows **everything the hash covers and that takes effect**: limits and fallback engines too (a role can
  name any engine of the owner's catalog, expensive ones included), which of the owner's roles would be replaced,
  and engines that do not exist.
-->
<script lang="ts">
  import { messageOf } from "../core/errors";
  import { confirmProjectRoles, projectRoles, type Limits, type ProjectRoles, type Role } from "../core/roster";
  import { toast } from "../core/toast";
  import Notice from "../ui/Notice.svelte";

  let { projectId }: { projectId: number | null } = $props();

  let found = $state<ProjectRoles | null>(null);
  let reviewing = $state(false);
  /** Counts requests, so a slow answer for a project that is no longer open cannot overwrite the current one. */
  let latest = 0;

  async function refresh() {
    const mine = ++latest;
    if (projectId === null) {
      found = null;
      return;
    }
    try {
      const answer = await projectRoles(projectId);
      if (mine === latest) found = answer;
    } catch (err) {
      if (mine !== latest) return;
      found = null;
      toast(`Project roles: ${messageOf(err)}`, "warning");
    }
  }

  $effect(() => {
    void projectId;
    reviewing = false;
    void refresh();
  });

  async function allow() {
    if (projectId === null || !found) return;
    try {
      await confirmProjectRoles(projectId, found.overrides.hash);
      reviewing = false;
    } catch (err) {
      toast(messageOf(err), "warning");
    }
    await refresh();
  }

  const limitsText = (limits: Limits): string =>
    [
      limits.max_cost_usd != null ? `$${limits.max_cost_usd}` : "",
      limits.max_tokens != null ? `${limits.max_tokens} tokens` : "",
      limits.max_steps != null ? `${limits.max_steps} steps` : "",
    ]
      .filter(Boolean)
      .join(", ");

  /** Whether a project's role still names an engine: a role chooses none any more (agents-model.md, Q19). */
  const namesEngine = (role: Role): boolean => role.engine !== null || role.fallback_engines.length > 0;
  const engineNames = (role: Role): string => [role.engine, ...role.fallback_engines].filter(Boolean).join(", ");

  /** One line per field that takes effect, so nothing the hash covers is hidden from the review. */
  const facts = (role: Role): string[] =>
    [
      role.description ? `description: ${role.description}` : "",
      `kind: ${role.kind}, tier: ${role.tier}`,
      namesEngine(role) ? `names an engine (${engineNames(role)}) — ignored: the engine of the tier runs` : "",
      role.creates.length > 0 ? `creates cards without asking: ${role.creates.join(", ")}` : "",
      role.permissions.length > 0
        ? `rights, granted without asking in sessions started for a card: ${role.permissions.join("; ")}`
        : "",
      limitsText(role.limits) ? `limits: ${limitsText(role.limits)}` : "limits: none of its own",
    ].filter(Boolean);
</script>

{#if found?.overrides.present}
  {#if found.confirmed}
    <p class="applied">
      Roles from this project apply (confirmed): {found.overrides.roles.map((r) => r.name).join(", ") || "none that could be read"}.
    </p>
  {:else}
    {#snippet review()}
      <ul class="review">
        {#each found?.overrides.roles ?? [] as role (role.name)}
          <li>
            <strong>{role.name}</strong>
            {#each facts(role) as line (line)}<br /><span class="muted">{line}</span>{/each}
            <pre>{role.instructions}</pre>
          </li>
        {/each}
      </ul>
      <div class="actions">
        <button type="button" class="ax-btn primary" onclick={() => void allow()}>Allow these roles</button>
        <button type="button" class="ax-btn" onclick={() => (reviewing = false)}>Cancel</button>
      </div>
    {/snippet}
    <div class="slot">
      <Notice
        title="Project roles need your review"
        alert
        children={reviewing && !found.overrides.blocked ? review : undefined}
      >
        {#snippet details()}
          <p>
            This project has its own agent roles in <code>.axiomata/agents/</code>. Their instructions are what an
            agent will obey — and the rights they list are granted without asking in a session started for a card,
            where nobody watches each step — so they stay off until you have read them.
          </p>
        {/snippet}
        {#snippet action()}
          {#if !found?.overrides.blocked && !reviewing}
            <button type="button" class="ax-btn" onclick={() => (reviewing = true)}>Review…</button>
          {/if}
        {/snippet}
      </Notice>
      <!-- What is wrong with them stays in sight, not folded away: it changes what "Allow" would mean. -->
      {#if found.overrides.blocked}
        <p class="problem">They cannot be confirmed: {found.overrides.blocked}.</p>
      {/if}
      {#each found.overrides.skipped as s (s.name)}
        <p class="problem">Skipped <code>{s.name}</code>: {s.reason}</p>
      {/each}
      {#if found.replaces.length > 0}
        <p class="problem">Replaces your own role{found.replaces.length === 1 ? "" : "s"}: {found.replaces.join(", ")}.</p>
      {/if}
      {#if found.overrides.roles.some(namesEngine)}
        <p class="hint">
          {found.overrides.roles.filter(namesEngine).map((r) => r.name).join(", ")} name{found.overrides.roles.filter(namesEngine).length === 1 ? "s" : ""}
          an engine. A role chooses no engine any more, so it is ignored: the engine of the role's tier runs.
        </p>
      {/if}
    </div>
  {/if}
{/if}

<style>
  .slot {
    margin-bottom: var(--ax-space-3);
  }
  .applied {
    margin: 0 0 var(--ax-space-2);
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
  }
  .problem {
    margin: var(--ax-space-1) 0 0;
    padding-left: var(--ax-space-3);
    color: var(--ax-warning);
    font: var(--ax-type-caption);
  }
  .hint {
    margin: var(--ax-space-1) 0 0;
    padding-left: var(--ax-space-3);
    color: var(--ax-text-muted);
    font: var(--ax-type-caption);
  }
  .muted {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .review {
    margin: 0 0 var(--ax-space-2);
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    word-break: break-word;
  }
  pre {
    margin: var(--ax-space-1) 0 0;
    padding: var(--ax-space-2);
    max-height: calc(160px * var(--ax-ui-scale));
    overflow: auto;
    white-space: pre-wrap;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    background: var(--ax-bg);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }
  .actions {
    display: flex;
    gap: var(--ax-space-2);
  }
  code {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }
</style>

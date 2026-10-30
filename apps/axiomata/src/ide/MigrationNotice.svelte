<!--
  What the one-time agents migration did and what it left for the owner (`docs/plans/agents-model.md`, AM6, Q14): the
  result in one line, and each role it kept because it differs from its base — side by side — with "Keep as own role"
  or "Fold into <base>" (into the backup folder; nothing is deleted).
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { messageOf } from "../core/errors";
  import { toast } from "../core/toast";
  import Notice from "../ui/Notice.svelte";
  import {
    agentsMigrationState,
    dismissMigrationNotice,
    fieldText,
    foldRoleVariant,
    keepRoleVariant,
    migrationSummary,
    type MigrationView,
  } from "./agentsMigration";

  /** The roles changed: the list beside reads them again. */
  let { onChange = () => {} }: { onChange?: () => void } = $props();

  let view = $state<MigrationView | null>(null);
  let busy = $state<string | null>(null);

  async function reload() {
    try {
      view = await agentsMigrationState();
    } catch {
      view = null;
    }
  }

  async function act(name: string, step: () => Promise<void>) {
    busy = name;
    try {
      await step();
      await reload();
      onChange();
    } catch (err) {
      toast(messageOf(err), "warning");
    } finally {
      busy = null;
    }
  }

  onMount(() => void reload());
</script>

{#if view?.report && !view.notice_seen}
  {@const report = view.report}
  <div class="slot">
    <Notice tone="ok" title={migrationSummary(report)}>
      {#snippet details()}
        {#if report.backup}
          <p>The originals are in <code>{report.backup}</code>; delete it in Settings → Storage.</p>
        {/if}
        {#if Object.keys(report.defaults_seeded).length > 0}
          <p>
            Default engines per tier:
            {Object.entries(report.defaults_seeded)
              .map(([tier, engine]) => `${tier} ${engine}`)
              .join(", ")}.
          </p>
        {/if}
      {/snippet}
      {#snippet action()}
        <button type="button" class="ax-btn" onclick={() => void act("notice", dismissMigrationNotice)}>OK</button>
      {/snippet}
    </Notice>
  </div>
{/if}

{#if view && view.pending.length > 0}
  <div class="slot">
    <Notice title="{view.pending.length} role{view.pending.length === 1 ? '' : 's'} differ from their base">
      {#snippet details()}
        <p>
          These were copies of a base role for another model. They say something of their own, so they stayed. Keep
          each as an own role, or fold it into its base (it goes into the backup folder).
        </p>
      {/snippet}
      {#snippet children()}
        <ul class="variants">
          {#each view?.pending ?? [] as diff (diff.variant.name)}
            <li>
              <div class="head">
                <span class="name">{diff.variant.name}</span>
                <span class="muted">differs from {diff.base.name} in {diff.differs.join(", ")}</span>
              </div>
              <details>
                <summary>Side by side</summary>
                {#each diff.differs as field (field)}
                  <div class="field-name">{field}</div>
                  <div class="sides">
                    <pre><strong>{diff.base.name}</strong>
{fieldText(diff.base, field)}</pre>
                    <pre><strong>{diff.variant.name}</strong>
{fieldText(diff.variant, field)}</pre>
                  </div>
                {/each}
              </details>
              <div class="actions">
                <button
                  type="button"
                  class="ax-btn"
                  disabled={busy !== null}
                  onclick={() => void act(diff.variant.name, () => keepRoleVariant(diff.variant.name))}
                  >Keep as own role</button
                >
                <button
                  type="button"
                  class="ax-btn"
                  disabled={busy !== null}
                  onclick={() => void act(diff.variant.name, () => foldRoleVariant(diff.variant.name))}
                  >Fold into {diff.base.name}</button
                >
              </div>
            </li>
          {/each}
        </ul>
      {/snippet}
    </Notice>
  </div>
{/if}

<style>
  .slot {
    margin-bottom: var(--ax-space-3);
  }

  .slot code {
    font-family: var(--ax-font-mono);
  }

  .variants {
    list-style: none;
    margin: var(--ax-space-2) 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
  }

  .variants li {
    padding: var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-2);
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    gap: 0 var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }

  .name {
    font-weight: 600;
  }

  .muted,
  summary,
  .field-name {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }

  summary {
    cursor: pointer;
    margin: var(--ax-space-1) 0;
  }

  .field-name {
    margin-top: var(--ax-space-2);
  }

  .sides {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--ax-space-2);
  }

  .sides pre {
    margin: 0;
    padding: var(--ax-space-2);
    max-height: calc(220px * var(--ax-ui-scale));
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
    margin-top: var(--ax-space-2);
  }
</style>

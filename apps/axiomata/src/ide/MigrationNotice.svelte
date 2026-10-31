<!--
  What the one-time agents migration did (`docs/plans/agents-model.md`, AM6, Q14): the result in one line, and where
  the originals went — every role variant is in the backup folder; nothing is deleted.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { messageOf } from "../core/errors";
  import { toast } from "../core/toast";
  import { tAgents as t } from "../i18n/agents";
  import Notice from "../ui/Notice.svelte";
  import {
    agentsMigrationState,
    dismissMigrationNotice,
    migrationSummary,
    type MigrationView,
  } from "./agentsMigration";

  /** The roles changed: the list beside reads them again. */
  let { onChange = () => {} }: { onChange?: () => void } = $props();

  let view = $state<MigrationView | null>(null);

  async function reload() {
    try {
      view = await agentsMigrationState();
    } catch {
      view = null;
    }
  }

  async function dismiss() {
    try {
      await dismissMigrationNotice();
      await reload();
      onChange();
    } catch (err) {
      toast(messageOf(err), "warning");
    }
  }

  onMount(() => void reload());
</script>

{#if view?.problem}
  <div class="slot">
    <Notice tone="bad" title={t("migrationFailed")} alert>
      {#snippet details()}
        <p>{view?.problem}</p>
      {/snippet}
    </Notice>
  </div>
{/if}

{#if view?.report && !view.notice_seen}
  {@const report = view.report}
  <div class="slot">
    <Notice tone="ok" title={migrationSummary(report)}>
      {#snippet details()}
        {#if report.backup}
          <p>{t("migrationBackupBefore")} <code>{report.backup}</code>{t("migrationBackupAfter")}</p>
        {/if}
        {#if report.skipped.length > 0}
          <p>{t("migrationLeft", { items: report.skipped.join("; ") })}</p>
        {/if}
        {#if Object.keys(report.defaults_seeded).length > 0}
          <p>
            {t("migrationDefaults", {
              items: Object.entries(report.defaults_seeded)
                .map(([tier, engine]) => `${tier} ${engine}`)
                .join(", "),
            })}
          </p>
        {/if}
      {/snippet}
      {#snippet action()}
        <button type="button" class="ax-btn" onclick={() => void dismiss()}>{t("ok")}</button>
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
</style>

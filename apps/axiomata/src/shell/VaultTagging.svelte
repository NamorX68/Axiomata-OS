<!--
  The one run over the whole vault (`docs/plans/tags-links.md` Q9, TL6), at the end of Settings → Workspace → Tags.
  Two steps, each started by the owner after the estimate of what it reads and costs: (1) the skill `vault-tagging`
  proposes a tag list — the proposals wait above, as any other; (2) it plans tags and related links for every note
  into a preview. Nothing is written into a note until the owner writes the preview (or throws it away).
-->
<script lang="ts">
  import { messageOf } from "../core/errors";
  import { onMount } from "svelte";

  import { invokeBackend, type RunRecord, type TagProposal, type VaultApplied, type VaultPhase, type VaultTagging } from "../core/backend";
  import { refreshTagProposals } from "../core/tagProposals";
  import { toast } from "../core/toast";
  import { estimateLine } from "../core/vaultTagging";
  import { tTags } from "../i18n/tags";
  import StatusMark from "../ui/StatusMark.svelte";

  let { onChanged }: { onChanged?: () => void } = $props();

  let status = $state<VaultTagging | null>(null);
  let error = $state<string | null>(null);
  /** The step this window started and waits for; a step marked under way without it was left by a closed app. */
  let running = $state<VaultPhase | null>(null);
  let busy = $state(false);

  /** Rows of the preview shown before "show all". */
  const PREVIEW_ROWS = 30;
  let showAll = $state(false);
  const rows = $derived(status ? (showAll ? status.plan : status.plan.slice(0, PREVIEW_ROWS)) : []);
  const stale = $derived(status?.phase !== null && status?.phase !== undefined && running === null);

  async function load() {
    try {
      status = await invokeBackend<VaultTagging>("vault_tagging_status");
      error = null;
    } catch (err) {
      error = messageOf(err);
    }
  }

  async function start(phase: VaultPhase) {
    if (!status) return;
    const estimate = phase === "list" ? status.list_estimate : status.notes_estimate;
    const what = phase === "list" ? tTags("confirmList") : tTags("confirmNotes");
    if (!confirm(`${what}\n\n${estimateLine(estimate)}`)) return;
    running = phase;
    try {
      const run = await invokeBackend<RunRecord>("vault_tagging_start", { phase });
      toast(tTags("runStatus", { status: run.status }), run.status === "success" ? "info" : "warning");
    } catch (err) {
      toast(tTags("runFailed", { error: messageOf(err) }), "danger");
    } finally {
      running = null;
      await refreshTagProposals(() => invokeBackend<TagProposal[]>("tag_proposals"));
      await load();
      onChanged?.();
    }
  }

  async function apply() {
    if (!status || status.plan.length === 0) return;
    if (!confirm(tTags("confirmApply", { n: status.plan.length }))) return;
    busy = true;
    try {
      const done = await invokeBackend<VaultApplied>("vault_tagging_apply");
      const written = done.unchanged
        ? tTags("writtenUnchanged", { n: done.written, unchanged: done.unchanged })
        : tTags("written", { n: done.written });
      toast(written, "info");
      // The failed notes stay in the preview for another try; a few are named here.
      if (done.failed.length > 0) {
        toast(tTags("notWritten", { n: done.failed.length, first: done.failed[0] }), "warning");
      }
      await load();
    } catch (err) {
      toast(tTags("writeFailed", { error: messageOf(err) }), "danger");
    } finally {
      busy = false;
    }
  }

  async function discard() {
    if (!confirm(tTags("confirmDiscard"))) return;
    busy = true;
    try {
      await invokeBackend<void>("vault_tagging_discard");
      await load();
    } finally {
      busy = false;
    }
  }

  async function endStale() {
    busy = true;
    try {
      await invokeBackend<void>("vault_tagging_end");
      await load();
    } finally {
      busy = false;
    }
  }

  onMount(load);
</script>

<h3 class="sub">{tTags("vaultTitle")}</h3>
{#if error}
  <p class="status error">{error}</p>
{:else if status}
  <p class="hint">{tTags("vaultHint")}</p>
  <ol class="steps">
    <li>
      <div class="step-head">
        <span class="title">{tTags("stepList")}</span>
        {#if running === "list"}<StatusMark tone="live" label={tTags("running")} />{/if}
        <button type="button" disabled={running !== null || busy || stale} onclick={() => void start("list")}>
          {running === "list" ? tTags("proposing") : tTags("propose")}
        </button>
      </div>
      <p class="meta">{estimateLine(status.list_estimate)}</p>
    </li>
    <li>
      <div class="step-head">
        <span class="title">{tTags("stepNotes")}</span>
        {#if running === "notes"}<StatusMark tone="live" label={tTags("running")} />{/if}
        <button type="button" disabled={running !== null || busy || stale} onclick={() => void start("notes")}>
          {running === "notes" ? tTags("planning") : status.plan.length > 0 ? tTags("planRest") : tTags("plan")}
        </button>
      </div>
      <p class="meta">{estimateLine(status.notes_estimate)}</p>
    </li>
  </ol>
  {#if stale}
    <p class="hint">
      {tTags("stale")}
      <button type="button" class="inline" disabled={busy} onclick={() => void endStale()}>{tTags("endIt")}</button>
    </p>
  {/if}

  {#if status.plan.length > 0}
    <h3 class="sub">{tTags("preview", { n: status.plan.length })}</h3>
    <ul class="preview">
      {#each rows as row (row.note)}
        <li>
          <span class="note" title={row.note}>{row.note}</span>
          <span class="what">
            {#each row.tags as tag (tag)}<span class="chip">#{tag}</span>{/each}
            {#each row.related as name (name)}<span class="link">[[{name}]]</span>{/each}
            {#each row.proposed as tag (tag)}
              <span class="chip proposed" title={tTags("proposedChip")}>
                #{tag}?
              </span>
            {/each}
            {#if row.skipped}
              <span class="meta">{tTags("leftOut", { reason: row.skipped })}</span>
            {:else if row.tags.length === 0 && row.related.length === 0 && row.proposed.length === 0}
              <span class="meta">{tTags("nothing")}</span>
            {/if}
          </span>
        </li>
      {/each}
    </ul>
    {#if status.plan.length > PREVIEW_ROWS && !showAll}
      <button type="button" class="inline" onclick={() => (showAll = true)}>
        {tTags("showAll", { n: status.plan.length })}
      </button>
    {/if}
    <div class="actions">
      <button type="button" class="primary" disabled={busy || running !== null} onclick={() => void apply()}>
        {tTags("writeInto", { n: status.plan.length })}
      </button>
      <button type="button" disabled={busy || running !== null} onclick={() => void discard()}>
        {tTags("discard")}
      </button>
    </div>
  {/if}
{/if}

<style>
  h3.sub {
    font: var(--ax-type-label);
    color: var(--ax-text-muted);
    margin: var(--ax-space-4) 0 var(--ax-space-2);
  }
  .hint,
  .meta,
  .status {
    margin: 0 0 var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .status.error {
    color: var(--ax-danger);
  }
  .steps,
  .preview {
    list-style: none;
    margin: 0 0 var(--ax-space-3);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
  }
  .steps li {
    padding: var(--ax-space-2);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }
  .step-head {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }
  .step-head .title {
    flex: 1 1 auto;
    font-weight: 600;
  }
  .steps .meta {
    margin: var(--ax-space-1) 0 0;
  }
  .preview {
    max-height: calc(260px * var(--ax-ui-scale));
    overflow: auto;
  }
  .preview li {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }
  .note {
    flex: 0 1 calc(220px * var(--ax-ui-scale));
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .what {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1);
  }
  .chip {
    padding: 0 var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-3);
    font-size: var(--ax-font-size-xs);
  }
  .chip.proposed {
    border-style: dashed;
    color: var(--ax-text-muted);
  }
  .link {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
    color: var(--ax-text-muted);
  }
  .actions {
    display: flex;
    gap: var(--ax-space-2);
  }
  .inline {
    margin-left: var(--ax-space-1);
    font-size: var(--ax-font-size-sm);
  }
</style>

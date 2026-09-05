<!--
  mail — a curated view backed by the `mail-digest` skill. Same no-live-poll
  shape as `calendar`/`reminders` (mail data sits behind an MCP tool only an
  agent can reach, so every refresh is a real agent turn, never a timer) —
  this shell reads back whichever run happened most recently, however it
  happened: Skills Deck, a Routine, or this tile's own ↻ (a plain
  `run_skill` call under the hood, same mechanism).

  Unlike `calendar`/`reminders`, there is no full inbox listing at all —
  `mail-digest`'s SOP only ever reports messages it judged important, plus
  ones matching the topics configured on the settings face (see
  `core/mail.ts`'s `TOPICS_PATH` doc comment for how that reaches a skill
  with no runtime parameters). No write actions yet (no create/reply/
  delete/archive) — read and summarise only, for this first step.

  Clicking a row writes that message's full summary as a workspace note and
  opens it in the file viewer as a slide-in panel (`openMailSummary`); the
  row itself only ever shows a short, truncated preview of the same text.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { RunRecord, RunSummary } from "../core/backend";
  import { relativeTime } from "../core/format";
  import { EMPTY_MAIL_DIGEST, loadLatestMailDigest, MAIL_SKILL_NAME, openMailSummary, parseMailDigest, summaryPreview, type MailDigest, type MailItem } from "../core/mail";
  import type { ModuleContext } from "../core/types";

  let { ctx }: { ctx: ModuleContext } = $props();

  let digest = $state<MailDigest>(EMPTY_MAIL_DIGEST);
  let lastRun = $state<RunSummary | null>(null);
  let loading = $state(true);
  let running = $state(false);
  let error = $state("");
  let openingId = $state<string | null>(null);

  /** Applies a just-finished `run_skill` result (`refreshNow` only —
   *  `loadLatest` goes through `loadLatestMailDigest` instead, which
   *  already does this same mapping for the "find the latest run" path). */
  function applyFreshRun(run: RunRecord) {
    lastRun = run;
    if (run.status === "failed") {
      digest = EMPTY_MAIL_DIGEST;
      error = run.error ?? "Last run failed.";
    } else {
      try {
        digest = parseMailDigest(run.stdout);
        error = "";
      } catch (err) {
        digest = EMPTY_MAIL_DIGEST;
        error = String(err instanceof Error ? err.message : err);
      }
    }
  }

  async function loadLatest() {
    loading = true;
    try {
      const result = await loadLatestMailDigest(ctx.invoke);
      lastRun = result.run;
      digest = result.digest;
      error = result.error ?? "";
    } catch (err) {
      error = String(err);
    } finally {
      loading = false;
    }
  }

  async function refreshNow() {
    if (running) return;
    running = true;
    try {
      const full = await ctx.invoke<RunRecord>("run_skill", { name: MAIL_SKILL_NAME });
      applyFreshRun(full);
    } catch (err) {
      error = String(err);
    } finally {
      running = false;
    }
  }

  async function openSummary(item: MailItem) {
    if (openingId) return;
    openingId = item.id;
    try {
      await openMailSummary(ctx.invoke, item);
    } catch (err) {
      error = String(err instanceof Error ? err.message : err);
    } finally {
      openingId = null;
    }
  }

  onMount(() => void loadLatest());
</script>

<div class="mail">
  <div class="head">
    <span class="count">{digest.emails.length}</span>
    <span class="muted">curated</span>
    <span class="spacer"></span>
    <span class="muted last-run" title={lastRun ? `Last run: ${lastRun.started_at}` : "No run yet"}>
      {lastRun ? relativeTime(lastRun.started_at) : "never run"}
    </span>
    <button type="button" class="refresh" title="Run mail-digest now" aria-label="Refresh" disabled={running} onclick={() => void refreshNow()}>
      {running ? "…" : "↻"}
    </button>
  </div>

  {#if error}<p class="error">{error}</p>{/if}

  {#if loading}
    <p class="muted">Loading…</p>
  {:else if !lastRun}
    <p class="muted empty">
      No data yet — run <code>{MAIL_SKILL_NAME}</code> from the Skills Deck, schedule it as a
      Routine, or hit ↻ above.
    </p>
  {:else if digest.emails.length === 0}
    <p class="muted empty">Nothing important or topic-matched right now.</p>
  {:else}
    <ul class="emails">
      {#each digest.emails as item (item.id)}
        <li class="email">
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
          <div class="row" role="button" tabindex="0" onclick={() => void openSummary(item)} onkeydown={(e) => e.key === "Enter" && openSummary(item)}>
            <span class="chip" class:chip-topic={item.reason === "topic"}>
              {item.reason === "topic" ? item.topic : "wichtig"}
            </span>
            <span class="subject">{item.subject}</span>
            <span class="muted date">{relativeTime(item.date)}</span>
          </div>
          <div class="sender muted">{item.sender}</div>
          <div class="summary muted">
            {openingId === item.id ? "Öffne…" : summaryPreview(item.summary)}
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .mail {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    padding: var(--ax-space-3);
    gap: var(--ax-space-2);
  }

  .head {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    flex: 0 0 auto;
  }
  .count {
    font-weight: 700;
    color: var(--ax-accent);
  }
  .spacer {
    flex: 1 1 auto;
  }
  .last-run {
    white-space: nowrap;
  }
  .refresh {
    padding: 1px var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
  }

  .emails {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    min-height: 0;
    flex: 1 1 auto;
  }
  .email {
    padding: var(--ax-space-2) 0;
    border-top: 1px solid var(--ax-border);
  }
  .email:first-child {
    border-top: none;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    cursor: pointer;
  }
  .chip {
    flex: 0 0 auto;
    padding: 0 var(--ax-space-1);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-3);
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
    text-transform: uppercase;
    letter-spacing: var(--ax-tracking-wide);
    white-space: nowrap;
  }
  .chip-topic {
    background: var(--ax-accent-muted);
    color: var(--ax-accent);
  }
  .subject {
    flex: 1 1 auto;
    min-width: 0;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .date {
    flex: 0 0 auto;
    font-size: var(--ax-font-size-sm);
    white-space: nowrap;
  }
  .sender {
    font-size: var(--ax-font-size-sm);
  }
  .summary {
    font-size: var(--ax-font-size-sm);
    overflow-wrap: anywhere;
  }

  .muted {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-sm);
  }
  .empty {
    padding: var(--ax-space-2) 0;
  }
  .error {
    color: var(--ax-danger);
    font-size: var(--ax-font-size-sm);
    margin: 0;
  }
  p {
    margin: 0;
  }
  code {
    font-family: var(--ax-font-mono);
  }
</style>

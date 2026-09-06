<!--
  mail — a curated view backed by the `mail-digest` skill (or whatever this
  instance's settings face has renamed it to, see `resolveSkillName`). Same
  no-live-poll shape as `calendar`/`reminders` (mail data sits behind an MCP
  tool only an agent can reach, so every refresh is a real agent turn,
  never a timer) — but unlike those two, this tile *does* trigger one real
  run on its own: once, the moment it first mounts (app start, or the
  instant it's newly placed via Add Module), never on a repeating timer.
  Any *recurring* refresh is still exclusively a Routine's job. The ↻
  button is the same `run_skill` call, just user-triggered instead of
  mount-triggered.

  Unlike `calendar`/`reminders`, there is no full inbox listing at all —
  `mail-digest`'s SOP only ever reports messages it judged important, plus
  ones matching the topics configured on the settings face (see
  `core/mail.ts`'s `TOPICS_PATH` doc comment for how that reaches a skill
  with no runtime parameters). No write actions yet (no create/reply/
  delete/archive) — read and summarise only, for this first step.

  Every curated email gets its own workspace note the moment the digest is
  seen — mount, ↻, or a just-finished run (`writeAllMailSummaries`) — not
  only the ones clicked, so a mail that later drops out of the live mailbox
  (deleted, or aged out of the digest on a later run) still leaves a durable
  summary; the tile's own list still always reflects the *latest* run,
  never these notes. Clicking a row additionally opens its note in the file
  viewer as a slide-in panel (`openMailSummary`); the row itself only ever
  shows a short, truncated preview of the same text.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import type { RunRecord, RunSummary } from "../core/backend";
  import { relativeTime } from "../core/format";
  import { EMPTY_MAIL_DIGEST, loadLatestMailDigest, mailMix, MAIL_SKILL_NAME, openMailSummary, parseMailDigest, summaryPreview, writeAllMailSummaries, type MailDigest, type MailItem } from "../core/mail";
  import { resolveSkillName } from "../core/skillRun";
  import type { ModuleContext } from "../core/types";

  let { ctx }: { ctx: ModuleContext } = $props();
  // `ctx` is created once per mounted instance and never swapped.
  // svelte-ignore state_referenced_locally
  const config = ctx.config;

  let digest = $state<MailDigest>(EMPTY_MAIL_DIGEST);
  let lastRun = $state<RunSummary | null>(null);
  let loading = $state(true);
  let running = $state(false);
  let error = $state("");
  let openingId = $state<string | null>(null);

  const skillName = $derived(resolveSkillName($config, MAIL_SKILL_NAME));
  const importantItems = $derived(digest.emails.filter((e) => e.reason === "important"));
  const topicItems = $derived(digest.emails.filter((e) => e.reason === "topic"));
  const mix = $derived(mailMix(digest));

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
        // Best-effort durability: every curated mail gets its own note the
        // moment it's seen, not only the ones a user clicks (see
        // `writeAllMailSummaries`'s own doc comment for why). Fire-and-forget
        // — a slow or partially-failing write must never block the tile.
        void writeAllMailSummaries(ctx.invoke, digest.emails);
      } catch (err) {
        digest = EMPTY_MAIL_DIGEST;
        error = String(err instanceof Error ? err.message : err);
      }
    }
  }

  async function loadLatest() {
    loading = true;
    try {
      const result = await loadLatestMailDigest(ctx.invoke, skillName);
      lastRun = result.run;
      digest = result.digest;
      error = result.error ?? "";
      void writeAllMailSummaries(ctx.invoke, digest.emails);
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
      const full = await ctx.invoke<RunRecord>("run_skill", { name: skillName });
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

  // Show whatever's cached immediately (fast), then kick off one real run
  // in the background — the mount-time refresh this module's doc comment
  // describes. `refreshNow` already no-ops if a run is somehow already in
  // flight, so this can't double-fire.
  onMount(() => void loadLatest().then(refreshNow));
</script>

<div class="mail">
  <div class="head">
    <div class="stat">
      <span class="stat-num">{digest.emails.length}</span>
      <span class="stat-label">curated<br />last 3 days</span>
    </div>
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
  {:else if !lastRun && running}
    <p class="muted empty">Running <code>{skillName}</code> for the first time…</p>
  {:else if !lastRun}
    <p class="muted empty">
      No data yet — run <code>{skillName}</code> from the Skills Deck, schedule it as a
      Routine, or hit ↻ above.
    </p>
  {:else if digest.emails.length === 0}
    <p class="muted empty">Nothing important or topic-matched right now.</p>
  {:else}
    <div class="body">
      {#if importantItems.length > 0}
        <div class="section-label">Important</div>
        <ul class="emails">
          {#each importantItems as item (item.id)}
            {@render row(item)}
          {/each}
        </ul>
      {/if}

      {#if mix.length > 0}
        <div class="section-label">Today's mix</div>
        <div class="mix-bar">
          {#each mix as seg (seg.label)}
            <span class="seg" style:flex="{seg.count} {seg.count} 0" style:background={seg.color} title="{seg.label}: {seg.count}"></span>
          {/each}
        </div>
        <div class="mix-legend">
          {#each mix as seg (seg.label)}
            <span class="legend-item">
              <span class="dot" style:background={seg.color}></span>{seg.label}
              <b>{seg.count}</b>
            </span>
          {/each}
        </div>
      {/if}

      {#if topicItems.length > 0}
        <div class="section-label">By topic</div>
        <ul class="emails">
          {#each topicItems as item (item.id)}
            {@render row(item)}
          {/each}
        </ul>
      {/if}
    </div>
  {/if}
</div>

{#snippet row(item: MailItem)}
  <li class="email">
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
    <div class="row" role="button" tabindex="0" onclick={() => void openSummary(item)} onkeydown={(e) => e.key === "Enter" && openSummary(item)}>
      <span class="chip" class:chip-topic={item.reason === "topic"}>
        {item.reason === "topic" ? item.topic : "Important"}
      </span>
      <span class="subject">{item.subject}</span>
      <span class="muted date">{relativeTime(item.date)}</span>
    </div>
    <div class="sender muted">{item.sender}</div>
    <div class="summary muted">
      {openingId === item.id ? "Opening…" : summaryPreview(item.summary)}
    </div>
  </li>
{/snippet}

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
    align-items: flex-end;
    gap: var(--ax-space-2);
    flex: 0 0 auto;
  }
  .stat {
    display: flex;
    align-items: baseline;
    gap: var(--ax-space-2);
  }
  .stat-num {
    font-size: var(--ax-font-size-xl);
    font-weight: 700;
    color: var(--ax-accent);
    line-height: 1;
  }
  .stat-label {
    font-size: var(--ax-font-size-xs);
    line-height: 1.3;
    color: var(--ax-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--ax-tracking-wide);
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

  .body {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    overflow-y: auto;
    min-height: 0;
    flex: 1 1 auto;
  }
  .section-label {
    flex: 0 0 auto;
    font-size: var(--ax-font-size-xs);
    font-weight: 600;
    color: var(--ax-text-muted);
    text-transform: uppercase;
    letter-spacing: var(--ax-tracking-wide);
  }
  .section-label:not(:first-child) {
    margin-top: var(--ax-space-1);
  }

  .mix-bar {
    display: flex;
    height: 8px;
    border-radius: var(--ax-radius-sm);
    overflow: hidden;
    flex: 0 0 auto;
  }
  .seg {
    display: block;
  }
  .mix-legend {
    display: flex;
    flex-wrap: wrap;
    gap: var(--ax-space-1) var(--ax-space-3);
    flex: 0 0 auto;
  }
  .legend-item {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
    white-space: nowrap;
  }
  .legend-item b {
    color: var(--ax-text);
    font-weight: 600;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: 0 0 auto;
  }

  .emails {
    list-style: none;
    margin: 0;
    padding: 0;
    flex: 0 0 auto;
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

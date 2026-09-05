<!--
  mail-settings — the topics editor. Topics are the only thing configurable
  here, and deliberately live in a workspace file (`TOPICS_PATH`, see
  `core/mail.ts`'s doc comment) rather than this instance's own `config`:
  `mail-digest` is a real, fixed-SOP skill (schedulable by a Routine later,
  same as `calendar-digest`/`reminders-digest`) and has no way to receive a
  runtime parameter — it reads the topics file directly instead. This
  settings face is just a thin editor over that same file, not a second
  source of truth for it.
-->
<script lang="ts">
  import { onMount } from "svelte";

  import { loadTopics, MAIL_SKILL_NAME, saveTopics } from "../core/mail";
  import type { ModuleContext } from "../core/types";

  let { ctx }: { ctx: ModuleContext } = $props();

  let text = $state("");
  let loading = $state(true);
  let saving = $state(false);
  let saved = $state(false);
  let error = $state("");

  onMount(async () => {
    try {
      const topics = await loadTopics(ctx.invoke);
      text = topics.join("\n");
    } catch (err) {
      error = String(err instanceof Error ? err.message : err);
    } finally {
      loading = false;
    }
  });

  async function save() {
    if (saving) return;
    saving = true;
    saved = false;
    error = "";
    try {
      const topics = text
        .split("\n")
        .map((line) => line.trim())
        .filter((line) => line.length > 0);
      await saveTopics(ctx.invoke, topics);
      saved = true;
    } catch (err) {
      error = String(err instanceof Error ? err.message : err);
    } finally {
      saving = false;
    }
  }
</script>

<div class="settings">
  <p class="muted">
    Backed by the <code>{MAIL_SKILL_NAME}</code> skill — this tile only ever reads back that
    skill's last run, it never triggers one on a timer.
  </p>
  <p class="muted">
    Run it by hand from the Skills Deck, hit ↻ on this tile's front, or schedule it as a Routine
    for a periodic background refresh.
  </p>

  <label class="field" for="mail-topics">
    Topics (one per line) — mail matching any of these shows up alongside whatever the agent
    judges important on its own:
  </label>
  {#if loading}
    <p class="muted">Loading…</p>
  {:else}
    <textarea
      id="mail-topics"
      bind:value={text}
      spellcheck="false"
      placeholder={"Fotografie\nDevelopment\nKI/AI/LLM"}
      oninput={() => (saved = false)}
    ></textarea>
    {#if error}<p class="error">{error}</p>{/if}
    <div class="row">
      {#if saved}<span class="muted saved">Saved.</span>{/if}
      <span class="spacer"></span>
      <button type="button" class="primary" disabled={saving} onclick={save}>
        {saving ? "Saving…" : "Save"}
      </button>
    </div>
  {/if}
</div>

<style>
  .settings {
    padding: var(--ax-space-3);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    font-size: var(--ax-font-size-sm);
    height: 100%;
    min-height: 0;
  }
  .muted {
    color: var(--ax-text-muted);
    margin: 0;
  }
  code {
    font-family: var(--ax-font-mono);
  }
  .field {
    margin-top: var(--ax-space-1);
  }
  textarea {
    flex: 1 1 auto;
    min-height: 80px;
    resize: none;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    padding: var(--ax-space-2);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--ax-space-2);
    flex: 0 0 auto;
  }
  .spacer {
    flex: 1 1 auto;
  }
  .saved {
    font-size: var(--ax-font-size-sm);
  }
  .primary {
    background: var(--ax-accent);
    border-color: var(--ax-accent);
    color: var(--ax-text-invert);
    font-weight: 600;
    padding: 1px var(--ax-space-2);
  }
  .primary:hover:not(:disabled) {
    background: var(--ax-accent-hover);
  }
  .error {
    color: var(--ax-danger);
    margin: 0;
  }
  p {
    margin: 0;
  }
</style>

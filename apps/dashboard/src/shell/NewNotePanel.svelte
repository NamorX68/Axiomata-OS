<!--
  New note: a title + Markdown body, no path picker — on Save the agent
  decides which existing vault area (or Inbox) the note belongs in and what
  to call the file (`create_note`, backed by `axiomata_core::notes`; the
  agent only *decides*, the Rust side does the actual write, same split as
  `axiomata-cli import obsidian`). The saved note opens in the right-hand
  staged viewer so the result is immediately visible.
-->
<script lang="ts">
  import { emit } from "../core/bus";
  import { insideTauri, invokeBackend } from "../core/backend";
  import { toast } from "../core/toast";

  let { open = $bindable(false) }: { open?: boolean } = $props();

  let titleField = $state<HTMLInputElement | null>(null);
  let title = $state("");
  let content = $state("");
  let saving = $state(false);
  let error = $state("");

  const canSave = $derived(title.trim().length > 0 && !saving);

  // Focus the title field whenever the dialog opens — not the native
  // `autofocus` attribute (flagged by a11y lint), same as `shell:search`
  // focusing the assistant bar in `AssistantBar.svelte`.
  $effect(() => {
    if (open) titleField?.focus();
  });

  function reset() {
    title = "";
    content = "";
    error = "";
  }

  function close() {
    if (saving) return;
    open = false;
    reset();
  }

  async function save() {
    if (!canSave) return;
    saving = true;
    error = "";
    try {
      const rel = await invokeBackend<string>("create_note", { title: title.trim(), content });
      toast(`Saved to ${rel}.`);
      emit("open-file", { path: rel, mode: "read", from: "right" });
      open = false;
      reset();
    } catch (err) {
      error = String(err);
    } finally {
      saving = false;
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") close();
    else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      void save();
    }
  }
</script>

<svelte:window onkeydown={open ? onKeydown : undefined} />

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div class="overlay" onclick={close}>
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="new-note-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
    >
      <header>
        <h2 id="new-note-title">New note</h2>
        <button type="button" class="close" aria-label="Close" onclick={close}>×</button>
      </header>

      <div class="body">
        <input type="text" placeholder="Title" bind:value={title} bind:this={titleField} disabled={saving} />
        <textarea placeholder="Write your note in Markdown…" bind:value={content} rows="14" disabled={saving}
        ></textarea>
        <p class="hint">
          {insideTauri()
            ? "Saved into the vault — the agent picks the folder and file name for you."
            : "Browser preview: always files into Inbox/ (no agent here)."}
        </p>
        {#if error}<p class="error">{error}</p>{/if}
      </div>

      <footer>
        <button type="button" onclick={close} disabled={saving}>Cancel</button>
        <button type="button" class="primary" onclick={save} disabled={!canSave}>{saving ? "Saving…" : "Save"}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: var(--ax-z-dialog);
    display: grid;
    place-items: center;
    background: var(--ax-overlay);
  }
  .dialog {
    width: min(560px, calc(100vw - 2 * var(--ax-space-5)));
    max-height: calc(100vh - 2 * var(--ax-space-5));
    display: flex;
    flex-direction: column;
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border-strong);
    border-radius: var(--ax-radius-lg);
    box-shadow: var(--ax-shadow-pop);
  }
  header {
    display: flex;
    align-items: center;
    padding: var(--ax-space-3) var(--ax-space-4);
    border-bottom: 1px solid var(--ax-border);
  }
  h2 {
    flex: 1 1 auto;
    font-size: var(--ax-font-size-sm);
    letter-spacing: var(--ax-tracking-wide);
    text-transform: uppercase;
  }
  .close {
    width: 24px;
    height: 24px;
    padding: 0;
    line-height: 1;
    background: transparent;
    border-color: transparent;
    color: var(--ax-text-muted);
  }

  .body {
    overflow: auto;
    padding: var(--ax-space-3) var(--ax-space-4);
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
  }
  input[type="text"] {
    font-size: var(--ax-font-size-lg);
    font-weight: 600;
  }
  textarea {
    resize: vertical;
    min-height: 220px;
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-sm);
    line-height: 1.55;
  }
  .hint {
    margin: 0;
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  .error {
    margin: 0;
    font-size: var(--ax-font-size-sm);
    color: var(--ax-danger);
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--ax-space-2);
    padding: var(--ax-space-3) var(--ax-space-4);
    border-top: 1px solid var(--ax-border);
  }
  .primary:not(:disabled) {
    border-color: var(--ax-accent);
    background: var(--ax-accent);
    color: var(--ax-text-invert);
  }
</style>

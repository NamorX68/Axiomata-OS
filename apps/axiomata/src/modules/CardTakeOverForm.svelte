<!--
  "Übernehmen": the owner's second gate (A2A CP-A6b). The reviewer has signed the card off; this puts exactly the
  reviewed work into the project's main line as one commit, closes the card and cleans up the sessions made for it.
  Nothing is pushed. A conflict is undone, and the files that conflict are named here.
-->
<script lang="ts">
  import { untrack } from "svelte";

  import { invokeBackend as invoke, type BoardCard, type CardTakeOver } from "../core/backend";
  import { messageOf } from "../core/errors";
  import { toast } from "../core/toast";
  import { defaultTakeOverMessage } from "../ide/cardStart";

  let { card, onDone }: { card: BoardCard; onDone: () => void } = $props();

  // Proposed once, when the form opens: what the owner types after that is theirs, whatever the card does.
  let message = $state(untrack(() => defaultTakeOverMessage(card)));
  let error = $state("");
  let conflict = $state<string[]>([]);
  let busy = $state(false);

  async function takeOver(): Promise<void> {
    if (busy) return;
    busy = true;
    error = "";
    conflict = [];
    try {
      const result = await invoke<CardTakeOver>("take_over_card", { cardId: card.id, message: message.trim() || null });
      if (result.outcome === "conflict") {
        conflict = result.files;
        return;
      }
      toast(`Karte #${card.id} ist übernommen (${result.commit.slice(0, 8)}).`, "info");
      for (const note of result.cleanup) toast(`Nicht aufgeräumt: ${note}`, "warning");
      onDone();
    } catch (err) {
      error = messageOf(err);
    } finally {
      busy = false;
    }
  }
</script>

<form
  class="take-over"
  onsubmit={(event) => {
    event.preventDefault();
    void takeOver();
  }}
>
  <label>
    Commit-Nachricht
    <input bind:value={message} aria-label="Commit-Nachricht" />
  </label>
  <p class="hint">
    Übernommen wird genau der Stand, den der Reviewer gesehen hat, als ein Commit auf dem Hauptzweig des Projekts.
    Es wird nichts gepusht.
  </p>
  {#if conflict.length > 0}
    <p class="error" role="alert">
      Konflikt, nichts wurde verändert. Diese Dateien sind in Konflikt:
      {conflict.join(", ")}
    </p>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="row">
    <button class="ax-btn primary" type="submit" disabled={busy}>{busy ? "Übernimmt …" : "Übernehmen"}</button>
    <button class="ax-btn" type="button" onclick={onDone}>Abbrechen</button>
  </div>
</form>

<style>
  .take-over {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    margin: 0 0 var(--ax-space-3);
    padding: var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    font-size: var(--ax-font-size-sm);
  }

  label {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    color: var(--ax-text-muted);
  }

  input {
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-1);
    color: var(--ax-text);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    font: inherit;
  }

  .hint {
    margin: 0;
    color: var(--ax-text-muted);
  }

  .row {
    display: flex;
    gap: var(--ax-space-2);
  }

  .error {
    margin: 0;
    color: var(--ax-warning);
  }
</style>

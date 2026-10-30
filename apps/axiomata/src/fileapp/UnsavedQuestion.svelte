<!--
  "Unsaved changes" asked where something is being closed (`docs/plans/editor.md`,
  ED4, W4, W11): a file panel, a file app tab. For a file: Save, Discard,
  Cancel; for a new note not filed yet: File it, Discard, Cancel.
-->
<script lang="ts">
  import { tEditor } from "../i18n/editor";

  interface Props {
    /** What is being closed, for the sentence. */
    name: string;
    /** A new note: filing it is the way to keep it. */
    untitled: boolean;
    onSave: () => void;
    onDiscard: () => void;
    onCancel: () => void;
  }

  let { name, untitled, onSave, onDiscard, onCancel }: Props = $props();
</script>

<div class="ask" role="alertdialog" aria-label={tEditor("unsavedChanges")}>
  <span>{untitled ? tEditor("noteNotFiled") : tEditor("hasUnsaved", { name })}</span>
  <button type="button" class="strong" onclick={onSave}>{untitled ? tEditor("fileIt") : tEditor("save")}</button>
  <button type="button" onclick={onDiscard}>{tEditor("discard")}</button>
  <button type="button" onclick={onCancel}>{tEditor("cancel")}</button>
</div>

<style>
  .ask {
    display: flex;
    align-items: center;
    gap: var(--ax-space-3);
    padding: var(--ax-space-2) var(--ax-space-5);
    background: var(--ax-accent-muted);
    border-bottom: 1px solid var(--ax-border);
    font-size: var(--ax-font-size-sm);
  }

  span {
    flex: 1;
  }

  button {
    padding: var(--ax-space-1) var(--ax-space-3);
    background: var(--ax-surface-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-xs);
    cursor: pointer;
  }

  button.strong {
    border-color: var(--ax-accent);
    color: var(--ax-accent);
  }
</style>

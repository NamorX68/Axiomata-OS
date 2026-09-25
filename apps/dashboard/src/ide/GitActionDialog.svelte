<!--
  Asking before a git handgrip (`docs/plans/git-layer.md`, G3, G7, G12, G13,
  H6, H10, H13): discarding a file or a hunk, committing what the agent left,
  taking the agent's work over into the project folder. An overlay over the
  Diffs view it belongs to, not a window-wide modal — two agents' diffs can be
  open side by side, each with its own question.

  * **Discarding says what it does**: back to how the base has it, as an
    uncommitted change that can be seen (and undone) until someone commits.
    While the agent works it says so too (H13) — allowed, not hidden.
  * **The messages come prefilled** (H10) and stay editable; an empty one
    cannot be sent.
  * **A take-over conflict is an answer, not an error** (G9): it lists the
    files and says nothing was changed.
-->
<script lang="ts" module>
  import type { TakeOverMode } from "../core/backend";

  /** What is being asked. */
  export type GitAction =
    | { kind: "discard-file"; path: string }
    | { kind: "discard-hunk"; path: string; hunk: number; header: string }
    | { kind: "commit"; message: string }
    | { kind: "take-over"; message: string; mode: TakeOverMode; branch: string };
</script>

<script lang="ts">
  import { onMount } from "svelte";

  interface Props {
    action: GitAction;
    agentName: string;
    /** The agent is mid-turn: discarding is allowed but said out loud (H13). */
    working: boolean;
    /** The handgrip is running. */
    busy: boolean;
    /** Why the last attempt failed, or the files of a take-over conflict. */
    problem: string | null;
    conflictFiles?: string[] | null;
    onConfirm: (action: GitAction) => void;
    onCancel: () => void;
  }

  let { action, agentName, working, busy, problem, conflictFiles = null, onConfirm, onCancel }: Props = $props();

  // The dialog edits its own copy; `action` is only where it starts.
  // svelte-ignore state_referenced_locally
  let message = $state("message" in action ? action.message : "");
  // svelte-ignore state_referenced_locally
  let mode = $state<TakeOverMode>(action.kind === "take-over" ? action.mode : "squash");
  let messageField = $state<HTMLTextAreaElement | null>(null);
  let confirmButton = $state<HTMLButtonElement | null>(null);

  const needsMessage = $derived(action.kind === "commit" || action.kind === "take-over");
  const canConfirm = $derived(!busy && (!needsMessage || message.trim() !== ""));
  const CONFIRM_LABEL: Record<GitAction["kind"], string> = {
    "discard-file": "Discard",
    "discard-hunk": "Discard",
    commit: "Commit",
    "take-over": "Take over",
  };

  function confirm(): void {
    if (!canConfirm) return;
    if (action.kind === "commit") onConfirm({ ...action, message: message.trim() });
    else if (action.kind === "take-over") onConfirm({ ...action, message: message.trim(), mode });
    else onConfirm(action);
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onCancel();
    } else if (e.key === "Enter" && (e.metaKey || !needsMessage)) {
      e.preventDefault();
      e.stopPropagation();
      confirm();
    }
  }

  // The message when there is one to write, otherwise the button that acts.
  onMount(() => (messageField ?? confirmButton)?.focus());
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onkeydown={onKeydown}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="git-action-title">
    {#if action.kind === "discard-file" || action.kind === "discard-hunk"}
      <h3 id="git-action-title">
        {action.kind === "discard-file" ? "Discard this file's changes?" : "Discard this change?"}
      </h3>
      <p>
        <code>{action.path}</code>
        {#if action.kind === "discard-hunk"}<span class="header">{action.header}</span>{/if}
      </p>
      <p class="muted">
        It goes back to how the base has it — committed work included. The result is an uncommitted change in
        {agentName}'s worktree, so you can still see it here and undo it until it is committed.
      </p>
    {:else if action.kind === "commit"}
      <h3 id="git-action-title">Commit what {agentName} left uncommitted</h3>
      <label class="field">
        <span>Message</span>
        <textarea bind:value={message} rows="3" bind:this={messageField}></textarea>
      </label>
    {:else}
      <h3 id="git-action-title">Take {agentName}'s work over into {action.branch}</h3>
      <div class="modes" role="radiogroup" aria-label="How">
        <label>
          <input type="radio" name="mode" value="squash" bind:group={mode} />
          <span><strong>Squash</strong> — one new commit on {action.branch}</span>
        </label>
        <label>
          <input type="radio" name="mode" value="no_ff" bind:group={mode} />
          <span><strong>Merge (--no-ff)</strong> — keeps {agentName}'s commits</span>
        </label>
      </div>
      <label class="field">
        <span>Message</span>
        <textarea bind:value={message} rows="3" bind:this={messageField}></textarea>
      </label>
      <p class="muted">Never pushes. {agentName}'s branch moves onto the new {action.branch}; its diff is then empty.</p>
    {/if}

    {#if working && action.kind !== "take-over"}
      <p class="warn">{agentName} is working right now and may change these files again.</p>
    {/if}
    {#if conflictFiles && conflictFiles.length > 0}
      <div class="problem">
        <p>These files conflict, so nothing was changed:</p>
        <ul>
          {#each conflictFiles as file (file)}<li><code>{file}</code></li>{/each}
        </ul>
      </div>
    {:else if problem}
      <p class="problem">{problem}</p>
    {/if}

    <div class="buttons">
      <button type="button" onclick={onCancel}>Cancel</button>
      <button
        type="button"
        class="primary"
        class:danger={action.kind === "discard-file" || action.kind === "discard-hunk"}
        disabled={!canConfirm}
        onclick={confirm}
        bind:this={confirmButton}
      >
        {busy ? "Working…" : CONFIRM_LABEL[action.kind]}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: absolute;
    inset: 0;
    z-index: 5;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: var(--ax-space-5) var(--ax-space-3);
    background: color-mix(in srgb, var(--ax-bg) 60%, transparent);
  }

  .dialog {
    width: min(460px, 100%);
    padding: var(--ax-space-4);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-md);
    background: var(--ax-surface-2);
    box-shadow: var(--ax-shadow-pop);
    color: var(--ax-text);
    font-size: var(--ax-font-size-sm);
  }

  h3 {
    margin: 0 0 var(--ax-space-3);
    font-size: var(--ax-font-size-base);
    font-weight: normal;
  }

  p {
    margin: 0 0 var(--ax-space-2);
  }

  code,
  .header {
    font-family: var(--ax-font-mono);
    font-size: var(--ax-font-size-xs);
  }

  .header {
    margin-left: var(--ax-space-2);
    color: var(--ax-text-muted);
  }

  .muted {
    color: var(--ax-text-muted);
  }

  .warn {
    color: var(--ax-warning);
  }

  .problem {
    color: var(--ax-danger);
  }

  .problem ul {
    margin: var(--ax-space-1) 0 var(--ax-space-2);
    padding-left: var(--ax-space-4);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    margin-bottom: var(--ax-space-3);
    color: var(--ax-text-muted);
  }

  textarea {
    padding: var(--ax-space-2);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    background: var(--ax-surface-1);
    color: var(--ax-text);
    font-family: var(--ax-font-sans);
    font-size: var(--ax-font-size-sm);
    resize: vertical;
  }

  textarea:focus {
    outline: none;
    border-color: var(--ax-accent);
  }

  .modes {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    margin-bottom: var(--ax-space-3);
  }

  .modes label {
    display: flex;
    gap: var(--ax-space-2);
    align-items: baseline;
    cursor: pointer;
  }

  .modes strong {
    font-weight: normal;
    color: var(--ax-text);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: var(--ax-space-2);
    margin-top: var(--ax-space-3);
  }

  .buttons button {
    padding: var(--ax-space-1) var(--ax-space-3);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-pill);
    background: var(--ax-surface-1);
    color: var(--ax-text);
    font: inherit;
    cursor: pointer;
  }

  .buttons button.primary {
    border-color: var(--ax-accent);
    color: var(--ax-accent);
  }

  .buttons button.primary.danger {
    border-color: var(--ax-danger);
    color: var(--ax-danger);
  }

  .buttons button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>

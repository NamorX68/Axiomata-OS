<!--
  A session's conversation as the owner reads it (A2A CP-A9): what reached the session and what it wrote, oldest first, and
  a line to write to it. Looking marks nothing read — that is the agent's own `read_inbox`. A message the owner writes lands
  in the session's inbox about its card; the studio announces it in the terminal once the session waits for input.
-->
<script lang="ts">
  import { invokeBackend as invoke, type MailLine, type OwnerSend } from "../core/backend";
  import { messageOf } from "../core/errors";
  import { relativeTime } from "../core/format";
  import { toast } from "../core/toast";

  let { agentId, visible = true }: { agentId: number; visible?: boolean } = $props();

  /** How often the conversation is read while shown: a message is rare, a read is one small query. */
  const POLL_MS = 4000;

  let lines = $state<MailLine[]>([]);
  let draft = $state("");
  let busy = $state(false);
  let error = $state("");

  async function read(id: number): Promise<MailLine[]> {
    return invoke<MailLine[]>("ide_mailbox_messages", { id });
  }

  $effect(() => {
    const id = agentId;
    if (!visible) return;
    let stale = false;
    const load = (): void => {
      read(id)
        .then((list) => {
          if (stale) return;
          lines = list;
          error = "";
        })
        .catch((err: unknown) => {
          if (!stale) error = messageOf(err);
        });
    };
    load();
    const timer = setInterval(load, POLL_MS);
    return () => {
      stale = true;
      clearInterval(timer);
    };
  });

  async function send(): Promise<void> {
    const text = draft.trim();
    if (text === "" || busy) return;
    busy = true;
    error = "";
    try {
      const result = await invoke<OwnerSend>("ide_mailbox_send", { id: agentId, message: text });
      draft = "";
      if (result.outcome === "undeliverable") {
        toast(`Gespeichert, aber niemand liest es: ${result.reason}`, "warning");
      }
      lines = await read(agentId);
    } catch (err) {
      error = messageOf(err);
    } finally {
      busy = false;
    }
  }

  function onKey(event: KeyboardEvent): void {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void send();
    }
  }
</script>

<section class="mail" aria-label="Messages">
  <h4>Messages</h4>
  {#if lines.length === 0}
    <p class="muted">No messages yet.</p>
  {:else}
    <ul>
      {#each lines as line (`${line.incoming ? "in" : "out"}-${line.id}`)}
        <li class:incoming={line.incoming} class:notice={line.kind === "notice"}>
          <span class="meta">
            <strong>{line.from}</strong> → {line.to}
            <span class="muted">· {relativeTime(line.at)}</span>
            {#if line.incoming && !line.read}<span class="unread">ungelesen</span>{/if}
            {#if line.status !== "delivered"}<span class="unread">{line.status}</span>{/if}
          </span>
          <span class="text">{line.text}</span>
        </li>
      {/each}
    </ul>
  {/if}
  <div class="write">
    <textarea
      rows="2"
      placeholder="Write to the session … (⌘↩ sends)"
      bind:value={draft}
      onkeydown={onKey}
      disabled={busy}
    ></textarea>
    <button class="ax-btn" type="button" disabled={busy || draft.trim() === ""} onclick={() => void send()}>Send</button>
  </div>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .mail {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
  }
  h4 {
    margin: 0;
    font-size: var(--ax-font-size-sm);
    color: var(--ax-text-muted);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-2);
    max-height: calc(220px * var(--ax-ui-scale));
    overflow-y: auto;
  }
  li {
    display: flex;
    flex-direction: column;
    gap: var(--ax-space-1);
    padding: var(--ax-space-1) var(--ax-space-2);
    background: var(--ax-surface-1);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
  }
  li.incoming {
    border-left: 3px solid var(--ax-accent);
  }
  li.notice {
    color: var(--ax-text-muted);
    font-style: italic;
  }
  .meta {
    font-size: var(--ax-font-size-xs);
  }
  .text {
    white-space: pre-wrap;
    word-break: break-word;
  }
  .muted {
    color: var(--ax-text-muted);
    font-size: var(--ax-font-size-xs);
  }
  .unread {
    margin-left: var(--ax-space-1);
    color: var(--ax-warning);
  }
  .write {
    display: flex;
    gap: var(--ax-space-2);
    align-items: flex-end;
  }
  textarea {
    flex: 1;
    resize: vertical;
    background: var(--ax-surface-1);
    color: var(--ax-text);
    border: 1px solid var(--ax-border);
    border-radius: var(--ax-radius-sm);
    padding: var(--ax-space-1) var(--ax-space-2);
    font: inherit;
  }
  .error {
    margin: 0;
    color: var(--ax-warning);
    word-break: break-word;
  }
</style>

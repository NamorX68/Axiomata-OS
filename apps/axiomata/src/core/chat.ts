/**
 * Assistant state: the in-memory transcript of the chat panel, the agent
 * session id (from the first reply), and the busy flag. Transcript lives
 * only as long as the app; "new session" clears it.
 */

import { get, writable } from "svelte/store";

import { type ChatMode, type ChatReply, invokeBackend } from "./backend";

export type TurnRole = "user" | "assistant" | "instruction" | "error";

export interface ChatTurn {
  id: number;
  role: TurnRole;
  /** Markdown for assistant/instruction replies, plain text otherwise. */
  text: string;
  at: number;
  costUsd?: number | null;
}

export const turns = writable<ChatTurn[]>([]);
export const sessionId = writable<string | null>(null);
export const busy = writable(false);
export const panelOpen = writable(false);

let nextId = 1;

function push(role: TurnRole, text: string, costUsd?: number | null): ChatTurn {
  const turn: ChatTurn = { id: nextId++, role, text, at: Date.now(), costUsd };
  turns.update((list) => [...list, turn]);
  return turn;
}

/** A local command's Markdown output, shown as an instruction-style turn. */
export function pushInstruction(markdown: string): void {
  push("instruction", markdown);
  panelOpen.set(true);
}

export function newSession(): void {
  turns.set([]);
  sessionId.set(null);
}

/** One turn against the agent. `chat` continues the panel's session;
 *  `instruct` is a one-shot instruction that also lands in the transcript. */
export async function send(message: string, mode: ChatMode): Promise<ChatReply | null> {
  if (get(busy)) return null;
  busy.set(true);
  panelOpen.set(true);
  push("user", mode === "instruct" ? `/${message}` : message);
  try {
    const reply = await invokeBackend<ChatReply>("assistant_send", {
      message,
      sessionId: get(sessionId),
      mode,
    });
    if (mode === "chat") sessionId.set(reply.session_id);
    push(
      reply.is_error ? "error" : mode === "instruct" ? "instruction" : "assistant",
      reply.reply_markdown || "(empty reply)",
      reply.cost_usd,
    );
    return reply;
  } catch (err) {
    push("error", String(err));
    return null;
  } finally {
    busy.set(false);
  }
}

/**
 * Assistant state: the in-memory transcript of the chat panel, the agent
 * session id (from the first reply), and the busy flag. Transcript lives
 * only as long as the app; "new session" clears it.
 */

import { get, writable } from "svelte/store";

import { type ChatMode, type ChatReply, type EffortLevel, invokeBackend } from "./backend";
import { messageOf } from "./errors";

export type TurnRole = "user" | "assistant" | "instruction" | "error";

export interface ChatTurn {
  id: number;
  role: TurnRole;
  /** Markdown for assistant/instruction replies, plain text otherwise. */
  text: string;
  at: number;
  costUsd?: number | null;
  /** The app actions the agent used for this reply (`ToolCall` labels), shown as hex chips above it. */
  tools?: ToolCall[];
}

/**
 * One action the agent called in the app while answering (`core/agent-bridge.ts`), shown as a hex chip instead of a
 * wall of text ("Glut + Hex", the thinking bar). Only these are visible to the app: the agent's own tools (MCP,
 * shell) run inside its harness and report nothing here.
 */
export interface ToolCall {
  label: string;
  ok: boolean;
}

export const turns = writable<ChatTurn[]>([]);
export const sessionId = writable<string | null>(null);
export const busy = writable(false);
export const panelOpen = writable(false);
/** The actions of the turn in progress. */
export const toolCalls = writable<ToolCall[]>([]);
/** The chat's plan switch (Glut 2, G12): on, a chat turn reads and plans but changes nothing; the owner switches it
 *  off to let the agent carry the plan out. Instructions are not affected. */
export const planMode = writable(false);
/** `/model <id>` (#108, S5): the chat provider's model this conversation runs on instead of its chat model; `null`
 *  is the configured one. A new conversation goes back to it. */
export const turnModel = writable<string | null>(null);
/** `/effort <level>` (`docs/plans/glut-effort.md`): how hard Glut's model reasons in this conversation; `null` is the
 *  configured level. Only Glut reads it; a new conversation goes back to the configured one. */
export const turnEffort = writable<EffortLevel | null>(null);

/** A Glut chat turn the app lost before it answered (durable runs): finished on the owner's click. */
export interface InterruptedTurn {
  run: string;
  session_id: string;
  title: string;
  started_at: string;
}

export const interruptedTurns = writable<InterruptedTurn[]>([]);

/** Reads the chat turns the app lost; a failure leaves the list as it was. */
export async function loadInterruptedTurns(): Promise<void> {
  try {
    interruptedTurns.set(await invokeBackend<InterruptedTurn[]>("interrupted_chat_turns"));
  } catch {
    // Nothing to offer then; the journals stay for the next look.
  }
}

/** Finishes a lost turn in its own conversation: the panel goes on with that conversation. */
export async function resumeTurn(turn: InterruptedTurn): Promise<ChatReply | null> {
  if (get(busy)) return null;
  // The conversation on screen comes back when the resume fails; the note stays until one succeeds.
  const shown = { turns: get(turns), session: get(sessionId) };
  turns.set([]);
  sessionId.set(turn.session_id);
  const reply = await send(turn.title, "chat", undefined, turn.run);
  if (reply && !reply.is_error) {
    interruptedTurns.update((list) => list.filter((t) => t.run !== turn.run));
  } else {
    const failed = get(turns).filter((t) => t.role === "error");
    turns.set([...shown.turns, ...failed]);
    sessionId.set(shown.session);
  }
  return reply;
}

/** Gives a lost turn up: its conversation stays as it was before the turn. */
export async function dropTurn(turn: InterruptedTurn): Promise<void> {
  try {
    await invokeBackend("discard_chat_turn", { run: turn.run });
  } finally {
    interruptedTurns.update((list) => list.filter((t) => t.run !== turn.run));
  }
}

/** A turn that is a command of the chat's harness, or a compaction (`assistant_send`'s `command`). */
export type ChatCommand = { kind: "run"; name: string } | { kind: "compact" };

let nextId = 1;

function push(role: TurnRole, text: string, costUsd?: number | null, tools?: ToolCall[]): ChatTurn {
  const turn: ChatTurn = { id: nextId++, role, text, at: Date.now(), costUsd, ...(tools?.length ? { tools } : {}) };
  turns.update((list) => [...list, turn]);
  return turn;
}

/**
 * Notes an app action an agent called; only while a chat turn is in progress — then it counts as that turn's. A bridge
 * request carries no session of its own, so an action another agent happens to call meanwhile lands on the chat's
 * reply too; skill runs no longer get the module manifest, which makes that rare.
 */
export function noteToolCall(label: string, ok: boolean): void {
  if (!get(busy)) return;
  toolCalls.update((list) => [...list, { label, ok }]);
}

/** A local command's Markdown output, shown as an instruction-style turn. */
export function pushInstruction(markdown: string): void {
  push("instruction", markdown);
  panelOpen.set(true);
}

export function newSession(): void {
  turns.set([]);
  sessionId.set(null);
  turnModel.set(null);
  turnEffort.set(null);
}

/** What the transcript shows for the owner's line. */
function typedLine(message: string, mode: ChatMode, command?: ChatCommand, resume?: string): string {
  if (resume) return `${message} (resumed after the app stopped)`;
  if (command?.kind === "compact") return "/compact";
  if (command?.kind === "run") return `/${command.name}${message ? ` ${message}` : ""}`;
  return mode === "instruct" ? `/do ${message}` : message;
}

/** One turn against the agent. `chat` continues the panel's session — as `plan` while the plan switch is on;
 *  `instruct` is a one-shot instruction that also lands in the transcript. A `command` makes the chat turn one of
 *  the harness's commands (`message` its arguments) or a compaction; `/model`'s and `/effort`'s choices ride along. */
export async function send(
  message: string,
  asked: ChatMode,
  command?: ChatCommand,
  resume?: string,
): Promise<ChatReply | null> {
  if (get(busy)) return null;
  const mode: ChatMode = asked === "chat" && get(planMode) ? "plan" : asked;
  busy.set(true);
  toolCalls.set([]);
  panelOpen.set(true);
  push("user", typedLine(message, mode, command, resume));
  try {
    const reply = await invokeBackend<ChatReply>("assistant_send", {
      message: resume ? "" : message,
      sessionId: get(sessionId),
      mode,
      model: get(turnModel),
      effort: get(turnEffort),
      command: command ?? null,
      resume: resume ?? null,
    });
    if (mode !== "instruct") sessionId.set(reply.session_id);
    push(
      reply.is_error ? "error" : mode === "instruct" ? "instruction" : "assistant",
      reply.reply_markdown || "(empty reply)",
      reply.cost_usd,
      get(toolCalls),
    );
    return reply;
  } catch (err) {
    push("error", messageOf(err));
    return null;
  } finally {
    busy.set(false);
  }
}

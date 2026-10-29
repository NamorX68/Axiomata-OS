/**
 * What the chat asks the owner before it happens (`docs/plans/chat-knows-app.md`, CH5, Q8): starting a skill, creating
 * or changing a routine, starting a card. An action marked `chat: "ask"` that the agent bridge receives lands here
 * instead of running; the owner answers in the app (`shell/ChatAsks.svelte`, the permission asks' pattern) and only a
 * yes runs it. The bridge answers the agent at once that the owner was asked — the agent never waits on a person, and
 * no answer is ever taken from anything but the owner's click.
 *
 * What the owner reads must be what runs (security audit): the whole detail is shown, and one too long to show cannot
 * be allowed; format characters (bidi overrides and the like) are dropped; a question goes stale after a while and is
 * asked once however often it comes.
 */

import { get, writable, type Writable } from "svelte/store";

import { toast } from "./toast";

/** One question waiting for the owner. Every text is plain text, never markup. */
export interface ChatAsk {
  id: number;
  /** The action as the owner reads it: "Run the skill mail-digest". */
  what: string;
  /** What else decides it — a routine's prompt, a card's body, a skill's description — in full, or cut. */
  detail: string;
  /** False when `detail` had to be cut: then the owner cannot allow it from here. */
  complete: boolean;
  /** The action's name, shown small. */
  source: string;
  /** Who the request says it comes from: `chat`, `agent:<name>`, … — informative, never trusted. */
  origin: string | null;
  /** When it was asked (ms). */
  at: number;
  run: () => Promise<unknown>;
}

/** What an action hands over to be asked. */
export interface AskInput {
  question: string;
  detail?: string;
  source: string;
  origin?: string | null;
}

/** Open questions at most: a chat that keeps asking is stopped rather than burying the screen. */
export const MAX_OPEN_ASKS = 5;
/** Longest question line; the rest is cut (the detail carries what matters in full). */
const MAX_WHAT = 300;
/** Longest detail shown in full; a longer one is shown cut and cannot be allowed. */
export const MAX_DETAIL = 4000;
/** How long a question stays answerable: a late yes would run something out of its moment. */
export const ASK_TTL_MS = 2 * 60 * 1000;
/** How long after it appears a question takes its first click: a stray click must not answer it. */
export const ARM_MS = 400;

export const chatAsks: Writable<ChatAsk[]> = writable([]);

let nextId = 1;

/** Text without format characters (bidi overrides, zero-width joiners), which can make a text read as another. */
function visible(text: string): string {
  return text.replace(/\p{Cf}/gu, "");
}

/** One line of at most {@link MAX_WHAT} characters. */
function oneLine(text: string): string {
  const line = visible(text).replace(/\s+/g, " ").trim();
  return line.length > MAX_WHAT ? `${line.slice(0, MAX_WHAT - 1)}…` : line;
}

/** Who asks, as the owner reads it. */
export function askerLabel(origin: string | null): string {
  if (origin === "chat") return "The chat asks";
  if (origin?.startsWith("agent:")) return `Agent ${oneLine(origin.slice("agent:".length))} asks`;
  return "A command asks";
}

/** Drops the questions older than {@link ASK_TTL_MS}; returns how many went. */
export function dropStale(now = Date.now()): number {
  const before = get(chatAsks).length;
  chatAsks.update((list) => list.filter((ask) => now - ask.at < ASK_TTL_MS));
  return before - get(chatAsks).length;
}

/** Queues a question for the owner, or finds the same one waiting. Throws when too many are open already. */
export function askOwner(input: AskInput, run: () => Promise<unknown>): { asked: true; question: string } {
  dropStale();
  const what = oneLine(input.question);
  const full = visible(input.detail ?? "").trim();
  const complete = full.length <= MAX_DETAIL;
  const detail = complete ? full : `${full.slice(0, MAX_DETAIL)}…`;
  const source = oneLine(input.source);
  const same = get(chatAsks).find((ask) => ask.what === what && ask.detail === detail && ask.source === source);
  if (same) return { asked: true, question: same.what };
  if (get(chatAsks).length >= MAX_OPEN_ASKS) {
    throw new Error(`the owner has ${MAX_OPEN_ASKS} questions open already; wait for their answers`);
  }
  const ask: ChatAsk = { id: nextId++, what, detail, complete, source, origin: input.origin ?? null, at: Date.now(), run };
  chatAsks.update((list) => [...list, ask]);
  return { asked: true, question: ask.what };
}

/**
 * The owner's answer: a yes runs the action, a no drops it. A question that is unknown (answered already), gone
 * stale, too long to have been shown in full, or answered before it was armed runs nothing.
 */
export async function answerChatAsk(id: number, allow: boolean, now = Date.now()): Promise<void> {
  const ask = get(chatAsks).find((entry) => entry.id === id);
  if (!ask) return;
  if (allow && now - ask.at < ARM_MS) return;
  chatAsks.update((list) => list.filter((entry) => entry.id !== id));
  if (!allow) {
    toast(`Declined: ${ask.what}`, "info");
    return;
  }
  if (now - ask.at >= ASK_TTL_MS) {
    toast(`Too late, nothing ran: ${ask.what}`, "warning");
    return;
  }
  if (!ask.complete) {
    toast(`Too long to show in full, nothing ran: ${ask.what}`, "warning");
    return;
  }
  try {
    await ask.run();
    toast(`Done: ${ask.what}`, "info");
  } catch (err) {
    toast(`${ask.what} failed: ${String(err)}`, "warning");
  }
}

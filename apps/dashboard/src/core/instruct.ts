/**
 * Shared "one-shot instruct write" infrastructure behind every connector
 * module's create/complete/delete actions (`calendar`, `reminders`, and
 * future ones). Unlike a digest read — a `SKILL.md`'s fixed SOP, run
 * unattended — a write needs per-call parameters a user just typed into a
 * tile's form, and skills have no way to take runtime parameters. So a
 * write goes through the same one-shot `assistant_send` instruct turn the
 * assistant bar's own `/` commands use, scoped to exactly the MCP tool the
 * write needs via `allowedTools` (see `AgentRequest.allowed_tools` in
 * `agents/mod.rs` for why that's required at all — an MCP tool call is
 * silently denied in a headless turn without it, regardless of permission
 * mode). This is a fresh, silent turn, not the visible chat panel's
 * `core/chat.ts` — a module write shouldn't pop the chat panel open or land
 * in its transcript, it should just report success or failure inline.
 *
 * Every instruction built by a connector module spells out the target MCP
 * tool's exact parameters directly, rather than describing the change in
 * prose — there's nothing left for the agent to interpret, which keeps a
 * real (if unavoidable) agent turn as fast and reliable as this kind of
 * turn can be. `runInstructWrite` then checks the reply actually says what
 * was asked for, rather than trusting a merely-non-error turn: neither
 * calendar nor reminders has a live poll to self-correct a stale local
 * patch, so a write whose underlying MCP tool call quietly failed (while
 * the *turn* itself completed fine) must not be read as a success.
 */

import type { ChatReply } from "./backend";
import type { Invoke } from "./skillRun";

/** What a successful write instruction should get back: an exact literal
 *  (`"OK"`, for a fire-and-forget delete/complete), or a freshly created
 *  item's own id (for create, so the caller can insert it into local
 *  state without a full digest re-run). */
export type ReplyExpectation = { kind: "literal"; value: string } | { kind: "id" };

/** One built instruction: the prose sent to the agent, plus what a
 *  successful reply should look like — kept together so
 *  `runInstructWrite` always verifies against the same expectation the
 *  instruction actually asked for. */
export interface ToolCallInstruction {
  message: string;
  expect: ReplyExpectation;
}

/** Wraps a value for inclusion in an instruct-turn instruction as a
 *  quoted, backslash-escaped string literal — defensive against a
 *  title/notes value that itself contains a quote or backslash. This
 *  guards the instruction's own syntax, not its semantics: a value crafted
 *  to look like more instructions is still just text the agent reads, not
 *  something a parser rejects the way SQL/shell escaping would. The
 *  `allowedTools` scope (one MCP tool, nothing else reachable) is what
 *  actually bounds the damage a hostile form value could do. */
export function quoteForInstruction(value: string): string {
  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

/**
 * Builds a "call this MCP tool with exactly these parameters" instruction —
 * the common shape behind every connector module's write action. `params`
 * are pre-formatted `key=value` fragments (build string values with
 * `quoteForInstruction`). The exact wording is a fixed template on
 * purpose: it's what makes the turn fast and reliable (nothing for the
 * agent to interpret), and it's stable enough for the dev-mock backend to
 * recognise and answer sensibly.
 */
export function buildToolCallInstruction(toolLabel: string, mcpToolName: string, params: string[], expect: ReplyExpectation): ToolCallInstruction {
  const replyWith = expect.kind === "literal" ? expect.value : "the created item's id";
  const message =
    `Call the ${toolLabel} MCP tool (${mcpToolName}) with exactly these parameters and nothing else: ` +
    `${params.join(", ")}. Do not ask for confirmation, do not explain. After it succeeds, reply with ` +
    `exactly ${replyWith} and nothing else.`;
  return { message, expect };
}

/** A reply that looks like a plausible tool-generated id rather than a
 *  short natural-language word a call that quietly failed might produce
 *  ("Error", "Failed", "None", "Denied", …). Real ids observed from the
 *  apple-reminders MCP server are UUID-shaped — a bare one for a reminder
 *  (`"11A9AA93-4EFF-..."`), two joined by a colon for a calendar event
 *  (`"452EF5AC-...:8D0E8E79-..."`) — so this rejects anything under 16
 *  characters or with no digit/hyphen/colon at all, on top of the basic
 *  "one unspaced token" shape. Not a format guarantee (the id shape isn't
 *  part of any documented contract), just enough to catch an obvious
 *  non-id reply that the older "any unspaced token" check let through. */
function looksLikePlausibleId(reply: string): boolean {
  if (reply.length < 16 || reply.length > 300 || /\s/.test(reply)) return false;
  return /[0-9]/.test(reply) || reply.includes("-") || reply.includes(":");
}

/**
 * Runs one instruct turn scoped to `allowedTools` and verifies the reply
 * actually matches `instruction.expect` — not just that the turn itself
 * didn't error. `is_error` reflects the agent *turn* (API error, ran out
 * of turns, …), not whether the MCP tool call inside it semantically
 * succeeded; a turn can complete cleanly while the agent's final text
 * explains it couldn't find that id. Since neither connector module polls,
 * there's nothing to self-correct a write the caller wrongly believed
 * succeeded, so this check is the only thing standing between a failed
 * write and a silently wrong local-state patch.
 *
 * Errors:
 *   Throws when the agent reports an error (`is_error`), when the reply
 *   doesn't match the expected literal, when an expected id doesn't look
 *   plausible, or when `invoke` itself rejects — every path funnels into
 *   one consistent failure for the caller's `catch`.
 */
export async function runInstructWrite(invoke: Invoke, instruction: ToolCallInstruction, allowedTools: string): Promise<string> {
  const reply = await invoke<ChatReply>("assistant_send", {
    message: instruction.message,
    sessionId: null,
    mode: "instruct",
    allowedTools,
  });
  if (reply.is_error) throw new Error(reply.reply_markdown.trim() || "The agent reported an error.");
  const text = reply.reply_markdown.trim();
  if (instruction.expect.kind === "literal") {
    if (text.toLowerCase() !== instruction.expect.value.toLowerCase()) {
      throw new Error(`The agent didn't confirm the write ("${text.slice(0, 200)}") — it may not have happened. Hit ↻ to check.`);
    }
    return text;
  }
  if (!looksLikePlausibleId(text)) {
    throw new Error("The item may have been created, but its id could not be confirmed — hit ↻ to check.");
  }
  return text;
}

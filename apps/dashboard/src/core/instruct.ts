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
 * turn can be.
 */

import type { ChatReply } from "./backend";
import type { Invoke } from "./skillRun";

/** Wraps a value for inclusion in an instruct-turn instruction as a
 *  quoted, backslash-escaped string literal — defensive against a
 *  title/notes value that itself contains a quote or backslash. */
export function quoteForInstruction(value: string): string {
  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

/**
 * Builds a "call this MCP tool with exactly these parameters" instruction —
 * the common shape behind every connector module's write action. `params`
 * are pre-formatted `key=value` fragments (build string values with
 * `quoteForInstruction`); `replyWith` is what the agent should send back
 * once the call succeeds — `"OK"` for a fire-and-forget write (delete,
 * complete), or a description like `"the created item's id"` when the
 * caller needs something back to update its own local state with.
 *
 * The exact wording is a fixed template on purpose: it's what makes the
 * turn fast and reliable (nothing for the agent to interpret), and it's
 * stable enough for the dev-mock backend to recognise and answer sensibly.
 */
export function buildToolCallInstruction(toolLabel: string, mcpToolName: string, params: string[], replyWith: string): string {
  return (
    `Call the ${toolLabel} MCP tool (${mcpToolName}) with exactly these parameters and nothing else: ` +
    `${params.join(", ")}. Do not ask for confirmation, do not explain. After it succeeds, reply with ` +
    `exactly ${replyWith} and nothing else.`
  );
}

/**
 * Runs one instruct turn scoped to `allowedTools` and returns its trimmed
 * reply text.
 *
 * Errors:
 *   Throws when the agent reports an error (`is_error`), or when
 *   `invoke` itself rejects (network/Tauri-layer failure) — either way
 *   the caller's own `catch` surfaces one consistent failure path.
 */
export async function runInstructWrite(invoke: Invoke, message: string, allowedTools: string): Promise<string> {
  const reply = await invoke<ChatReply>("assistant_send", {
    message,
    sessionId: null,
    mode: "instruct",
    allowedTools,
  });
  if (reply.is_error) throw new Error(reply.reply_markdown.trim() || "The agent reported an error.");
  return reply.reply_markdown.trim();
}

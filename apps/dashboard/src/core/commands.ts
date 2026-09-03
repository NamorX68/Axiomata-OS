/**
 * The `/` command router behind the assistant bar.
 *
 *   /add <type>                 place a module
 *   /remove <type|id-prefix>    remove the first matching instance
 *   /theme <id>                 switch theme
 *   /open <path> [right|bottom] stage a workspace Markdown file
 *   /newfile <path>             create an empty workspace file and stage it
 *   /skill run <name>           run a skill
 *   /brain [file path]          open the Second Brain view (optionally on a file)
 *   /<type> <action> [json]     call a module action on its first instance
 *   /help                       list the above
 *
 * Anything else after `/` is a one-shot agent instruction (`instruct`);
 * text without a leading `/` is a chat turn — both handled by the caller.
 */

import { get } from "svelte/store";

import { emit } from "./bus";
import { createInstance, destroyInstance } from "./lifecycle";
import { getModule, invokeAction, listModules } from "./registry";
import { openStaged, type StageFrom } from "./staging";
import { instances } from "./stores";
import { THEMES, applyTheme, isTheme } from "./themes";
import { invokeBackend } from "./backend";

export type Routed =
  | { kind: "chat"; message: string }
  | { kind: "instruct"; message: string }
  | { kind: "command"; name: string; args: string[] };

export interface CommandResult {
  ok: boolean;
  /** Short, toast-sized. */
  message: string;
  /** Optional Markdown for the chat panel (e.g. /help, action output). */
  detail?: string;
}

/** Classifies raw bar input. */
export function route(input: string): Routed | null {
  const text = input.trim();
  if (!text) return null;
  if (!text.startsWith("/")) return { kind: "chat", message: text };
  const [head, ...rest] = text.slice(1).split(/\s+/);
  const name = head.toLowerCase();
  if (isCommand(name)) return { kind: "command", name, args: rest };
  return { kind: "instruct", message: text.slice(1).trim() };
}

const SHELL_COMMANDS = ["add", "remove", "theme", "open", "newfile", "skill", "brain", "help"];

export function isCommand(name: string): boolean {
  return SHELL_COMMANDS.includes(name) || getModule(name) !== undefined;
}

export const HELP = `**Commands**

- \`/add <type>\` — place a module (${listTypes()})
- \`/remove <type|id>\` — remove the first matching tile
- \`/theme <id>\` — ${THEMES.map((t) => t.id).join(" · ")}
- \`/open <path> [right|bottom]\` — stage a workspace file
- \`/newfile <path>\` — create an empty workspace file and open it
- \`/skill run <name>\` — run a skill
- \`/brain [path]\` — open the Second Brain graph
- \`/<type> <action> [json]\` — call a module action, e.g. \`/memory-status sync\`
- anything else after \`/\` — one-shot agent instruction
- no slash — chat with the agent`;

function listTypes(): string {
  return listModules()
    .map((m) => `\`${m.type}\``)
    .join(", ");
}

export async function runCommand(name: string, args: string[]): Promise<CommandResult> {
  switch (name) {
    case "help":
      return { ok: true, message: "See the chat panel.", detail: HELP };

    case "add": {
      const type = args[0] ?? "";
      const r = createInstance(type);
      return r.ok
        ? { ok: true, message: `Added ${getModule(type)?.title ?? type}.` }
        : { ok: false, message: r.reason };
    }

    case "remove": {
      const key = args[0] ?? "";
      const hit = get(instances).find((i) => i.type === key || (key.length >= 4 && i.id.startsWith(key)));
      if (!hit) return { ok: false, message: `no tile matches "${key}"` };
      destroyInstance(hit.id);
      return { ok: true, message: `Removed ${getModule(hit.type)?.title ?? hit.type}.` };
    }

    case "theme": {
      const id = args[0] ?? "";
      if (!isTheme(id)) return { ok: false, message: `unknown theme "${id}" (${THEMES.map((t) => t.id).join(", ")})` };
      applyTheme(id);
      return { ok: true, message: `Theme: ${id}.` };
    }

    case "open": {
      const path = args[0];
      if (!path) return { ok: false, message: "usage: /open <path> [right|bottom]" };
      const from: StageFrom = args[1] === "bottom" ? "bottom" : "right";
      openStaged("md-file", { path, mode: "read" }, from);
      return { ok: true, message: `Opened ${path}.` };
    }

    case "newfile": {
      const path = args[0];
      if (!path) return { ok: false, message: "usage: /newfile <path>" };
      try {
        await invokeBackend("write_workspace_file", { rel: path, content: "" });
      } catch (err) {
        return { ok: false, message: String(err) };
      }
      openStaged("md-file", { path, mode: "edit" }, "right");
      return { ok: true, message: `Created ${path}.` };
    }

    case "brain": {
      const path = args.join(" ").trim();
      emit("open-second-brain", { focus: path ? `file:${path}` : null });
      return { ok: true, message: "Second Brain opened." };
    }

    case "skill": {
      if (args[0] !== "run" || !args[1]) return { ok: false, message: "usage: /skill run <name>" };
      try {
        const run = await invokeBackend<{ status: string; duration_ms: number }>("run_skill", { name: args[1] });
        return { ok: run.status === "success", message: `/${args[1]}: ${run.status} (${run.duration_ms} ms)` };
      } catch (err) {
        return { ok: false, message: String(err) };
      }
    }

    default: {
      // `/<type> <action> [json]` on the first instance of that type.
      const def = getModule(name);
      if (!def) return { ok: false, message: `unknown command /${name}` };
      const inst = get(instances).find((i) => i.type === name);
      if (!inst) return { ok: false, message: `no "${def.title}" tile on the canvas — /add ${name} first` };
      const action = args[0];
      if (!action) {
        const names = (def.actions ?? []).map((a) => `\`${a.name}\``).join(", ") || "none";
        return { ok: false, message: `usage: /${name} <action> [json] — actions: ${names}` };
      }
      let params: unknown = {};
      const raw = args.slice(1).join(" ");
      if (raw) {
        try {
          params = JSON.parse(raw);
        } catch {
          return { ok: false, message: "params must be JSON, e.g. {\"path\":\"notes/inbox.md\"}" };
        }
      }
      try {
        const out = await invokeAction(inst.id, action, params);
        const detail = out === undefined ? undefined : "```json\n" + JSON.stringify(out, null, 2) + "\n```";
        return { ok: true, message: `/${name} ${action} done.`, detail };
      } catch (err) {
        return { ok: false, message: String(err) };
      }
    }
  }
}

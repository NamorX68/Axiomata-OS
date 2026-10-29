/**
 * Frontend half of the agent ↔ module bridge (see axiomata_core::bridge):
 *
 * - whenever the instance list changes, the manifest of mounted modules and
 *   their actions is (debounced) written to ~/.axiomata/module-context.md via
 *   `write_module_manifest`, which the agent gets as appended system prompt;
 * - every POLL_MS the inbox queue is drained via `poll_module_actions`, each
 *   request is dispatched to `invokeAgentAction` (which asks the owner first or refuses what an action's `chat`
 *   mark says), and the outcome goes back through
 *   `complete_module_action`.
 */

import { get } from "svelte/store";

import { invokeBackend } from "./backend";
import { noteToolCall } from "./chat";
import { invokeAgentAction, manifest, type ManifestEntry } from "./registry";
import { instances } from "./stores";

export const POLL_MS = 3000;
export const MANIFEST_DEBOUNCE_MS = 500;

interface ActionRequest {
  id: string;
  instance_id: string;
  action: string;
  params: unknown;
  created_at: string;
  /** Who the request says it comes from (`chat`, `agent:<name>`, `terminal`); shown with a question, never trusted. */
  origin?: string | null;
}

interface ActionResponse {
  id: string;
  ok: boolean;
  result: unknown;
  error: string | null;
  completed_at: string;
}

let started = false;
let manifestTimer: ReturnType<typeof setTimeout> | null = null;
let lastManifestJson = "";

export function startAgentBridge(): () => void {
  if (started) return () => {};
  started = true;

  const unsubscribe = instances.subscribe(() => scheduleManifest());
  const poll = setInterval(() => void pollOnce(), POLL_MS);
  void pollOnce();

  return () => {
    started = false;
    unsubscribe();
    clearInterval(poll);
    if (manifestTimer) clearTimeout(manifestTimer);
  };
}

function scheduleManifest(): void {
  if (manifestTimer) clearTimeout(manifestTimer);
  manifestTimer = setTimeout(() => void writeManifest(), MANIFEST_DEBOUNCE_MS);
}

export async function writeManifest(): Promise<void> {
  const entries: ManifestEntry[] = manifest();
  const json = JSON.stringify(entries);
  if (json === lastManifestJson) return;
  try {
    await invokeBackend<boolean>("write_module_manifest", { entries });
    lastManifestJson = json;
  } catch (err) {
    console.warn("write_module_manifest failed:", err);
  }
}

/** An action as its chip in the chat says it: "calendar · create event". */
export function actionLabel(req: Pick<ActionRequest, "instance_id" | "action">): string {
  const type = get(instances).find((i) => i.id === req.instance_id)?.type ?? req.instance_id;
  return `${type} · ${req.action.replace(/_/g, " ")}`;
}

async function pollOnce(): Promise<void> {
  let requests: ActionRequest[];
  try {
    requests = await invokeBackend<ActionRequest[]>("poll_module_actions");
  } catch (err) {
    console.warn("poll_module_actions failed:", err);
    return;
  }
  for (const req of requests) {
    const response: ActionResponse = {
      id: req.id,
      ok: false,
      result: null,
      error: null,
      completed_at: "",
    };
    // Named before the action runs: an action that closes its own instance would leave only the raw id after it.
    const label = actionLabel(req);
    try {
      // An agent's call: its action's `chat` mark decides whether it runs, asks the owner first or is refused (CH5).
      const result = await invokeAgentAction(req.instance_id, req.action, req.params ?? {}, req.origin ?? null);
      response.result = result ?? null;
      response.ok = true;
    } catch (err) {
      response.error = String(err);
    }
    response.completed_at = new Date().toISOString();
    noteToolCall(label, response.ok);
    try {
      await invokeBackend("complete_module_action", { response });
    } catch (err) {
      console.warn("complete_module_action failed:", err);
    }
  }
}

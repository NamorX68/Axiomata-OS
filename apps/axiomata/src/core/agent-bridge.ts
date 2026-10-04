/**
 * Frontend half of the agent ↔ module bridge (see axiomata_core::bridge):
 *
 * - whenever the instance list changes, the manifest of mounted modules and
 *   their actions is (debounced) written to ~/.axiomata/module-context.md via
 *   `write_module_manifest`, which the agent gets as appended system prompt;
 * - every POLL_MS the inbox queue is drained via `poll_module_actions`, each
 *   request is dispatched to `invokeAction`, and the outcome goes back through
 *   `complete_module_action`.
 */

import { invokeBackend } from "./backend";
import { invokeAction, manifest, type ManifestEntry } from "./registry";
import { instances } from "./stores";

export const POLL_MS = 3000;
export const MANIFEST_DEBOUNCE_MS = 500;

interface ActionRequest {
  id: string;
  instance_id: string;
  action: string;
  params: unknown;
  created_at: string;
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
    try {
      response.result = (await invokeAction(req.instance_id, req.action, req.params ?? {})) ?? null;
      response.ok = true;
    } catch (err) {
      response.error = String(err);
    }
    response.completed_at = new Date().toISOString();
    try {
      await invokeBackend("complete_module_action", { response });
    } catch (err) {
      console.warn("complete_module_action failed:", err);
    }
  }
}

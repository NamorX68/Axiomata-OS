/**
 * Which mode the next opening of the workbench asks for (`docs/plans/workbench.md`, step 4): the
 * ring's "Editor" entry shows the workbench in the Editor mode, "IDE" in the Agents mode. The view
 * may not be mounted yet, so the request waits here and the view takes it once its project is in.
 */

import { writable } from "svelte/store";

import type { Mode } from "./modes";

export const modeRequest = writable<Mode | null>(null);

export function requestMode(mode: Mode): void {
  modeRequest.set(mode);
}

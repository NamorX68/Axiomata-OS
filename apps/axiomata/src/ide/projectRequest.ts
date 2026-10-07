/**
 * Which project the next opening of the workbench asks for (the spotlight's project hits). Like `modeRequest.ts`: the view
 * may not be mounted yet, so the request waits here and the view takes it once its own project is in.
 */

import { writable } from "svelte/store";

export const projectRequest = writable<number | null>(null);

export function requestProject(projectId: number): void {
  projectRequest.set(projectId);
}

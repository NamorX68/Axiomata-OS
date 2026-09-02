/**
 * Registers every built-in module. Called once from `main.ts`.
 *
 * Adding a module = a new file in this folder + one `registerModule(...)` line
 * here. Real modules land in steps 8–10; `dummy` is step-2 scaffolding.
 */

import { registerModule } from "../core/registry";
import Dummy from "./dummy.svelte";

export function registerBuiltins(): void {
  registerModule({
    type: "dummy",
    title: "Dummy",
    icon: "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor'><rect x='2.5' y='2.5' width='11' height='11' rx='2'/></svg>",
    component: Dummy,
    defaultSize: { w: 260, h: 160 },
    minSize: { w: 160, h: 100 },
    actions: [
      {
        name: "ping",
        description: "Return 'pong' — a no-op used to test the action pipeline.",
        params: { type: "object", properties: {} },
        run: async () => "pong",
      },
    ],
  });
}

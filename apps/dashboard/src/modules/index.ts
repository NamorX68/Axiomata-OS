/**
 * Registers every built-in module. Called once from `main.ts`.
 *
 * Adding a module = a new file in this folder + one `registerModule(...)` line
 * here. Real modules land in steps 8–10; the two `dummy*` entries are
 * dev-only scaffolding (a plain one and a singleton to exercise the guard).
 */

import { registerModule } from "../core/registry";
import Dummy from "./dummy.svelte";
import DummySettings from "./dummy-settings.svelte";

const DUMMY_ICON =
  "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor'><rect x='2.5' y='2.5' width='11' height='11' rx='2'/></svg>";

export function registerBuiltins(): void {
  if (import.meta.env.DEV) {
    registerModule({
      type: "dummy",
      title: "Dummy",
      icon: DUMMY_ICON,
      component: Dummy,
      settings: DummySettings,
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
    registerModule({
      type: "dummy-singleton",
      title: "Dummy (single)",
      icon: DUMMY_ICON,
      component: Dummy,
      defaultSize: { w: 220, h: 120 },
      singleton: true,
    });
  }
}

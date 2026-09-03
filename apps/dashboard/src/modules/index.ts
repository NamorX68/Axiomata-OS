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
import MemoryStatus from "./memory-status.svelte";
import MemoryStatusSettings from "./memory-status-settings.svelte";

const DUMMY_ICON =
  "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor'><rect x='2.5' y='2.5' width='11' height='11' rx='2'/></svg>";

export function registerBuiltins(): void {
  registerModule({
    type: "memory-status",
    title: "Memory",
    icon: "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor' stroke-width='1.4' stroke-linejoin='round'><path d='M3 2.5h7l3 3v8H3z'/><path d='M10 2.5v3h3M5.5 8.5h5M5.5 11h5'/></svg>",
    component: MemoryStatus,
    settings: MemoryStatusSettings,
    defaultSize: { w: 360, h: 150 },
    minSize: { w: 240, h: 90 },
    actions: [
      {
        name: "sync",
        description: "Regenerate the workspace CLAUDE.md router blocks now.",
        params: { type: "object", properties: {} },
        run: (_params, ctx) => ctx.invoke("sync_memory"),
      },
      {
        name: "status",
        description: "Return the router status (stale flag, tracked files, last sync).",
        params: { type: "object", properties: {} },
        run: (_params, ctx) => ctx.invoke("get_memory_status"),
      },
    ],
  });

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

/**
 * Registers every built-in module. Called once from `main.ts`.
 *
 * Adding a module = a new file in this folder + one `registerModule(...)` line
 * here. The two `dummy*` entries are dev-only scaffolding (a plain one and a
 * singleton to exercise the guard).
 */

import { registerModule } from "../core/registry";
import Dummy from "./dummy.svelte";
import DummySettings from "./dummy-settings.svelte";
import MdFile from "./md-file.svelte";
import MdFileSettings from "./md-file-settings.svelte";
import MemoryStatus from "./memory-status.svelte";
import MemoryStatusSettings from "./memory-status-settings.svelte";
import RoutinesBoard from "./routines-board.svelte";
import SecondBrain from "./second-brain.svelte";
import SecondBrainSettings from "./second-brain-settings.svelte";
import RoutinesBoardSettings from "./routines-board-settings.svelte";
import SkillsDeck from "./skills-deck.svelte";
import SkillsDeckSettings from "./skills-deck-settings.svelte";

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

  registerModule({
    type: "skills-deck",
    title: "Skills Deck",
    icon: "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor' stroke-width='1.4' stroke-linejoin='round'><path d='M9 1.5 3.5 9H8l-1 5.5L12.5 7H8z'/></svg>",
    component: SkillsDeck,
    settings: SkillsDeckSettings,
    defaultSize: { w: 420, h: 240 },
    minSize: { w: 220, h: 120 },
    actions: [
      {
        name: "run",
        description: "Run a skill by name and return its run summary.",
        params: { type: "object", properties: { skill: { type: "string" } }, required: ["skill"] },
        run: (params, ctx) => ctx.invoke("run_skill", { name: String((params as { skill: string }).skill) }),
      },
      {
        name: "list",
        description: "List the discovered skills.",
        params: { type: "object", properties: {} },
        run: (_params, ctx) => ctx.invoke("list_skills"),
      },
    ],
  });

  registerModule({
    type: "routines-board",
    title: "Routines",
    icon: "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor' stroke-width='1.4' stroke-linecap='round'><circle cx='8' cy='8' r='6'/><path d='M8 4.5V8l2.5 1.5'/></svg>",
    component: RoutinesBoard,
    settings: RoutinesBoardSettings,
    // Tall enough for the flip-side add-routine form without scrolling.
    defaultSize: { w: 440, h: 340 },
    minSize: { w: 280, h: 120 },
    actions: [
      {
        name: "add",
        description: "Create an enabled routine: cron (6–7 fields, seconds first), a skill name or a prompt.",
        params: {
          type: "object",
          properties: {
            name: { type: "string" },
            cron: { type: "string" },
            skill: { type: "string" },
            prompt: { type: "string" },
          },
          required: ["name", "cron"],
        },
        run: (params, ctx) => {
          const p = params as { name: string; cron: string; skill?: string; prompt?: string };
          const target = p.skill
            ? { type: "skill", value: p.skill }
            : { type: "prompt", value: p.prompt ?? "" };
          return ctx.invoke("add_routine", {
            new: { name: p.name, cron_expr: p.cron, target, backend: null, enabled: true },
          });
        },
      },
      {
        name: "setEnabled",
        description: "Enable or disable a routine by id.",
        params: {
          type: "object",
          properties: { id: { type: "integer" }, on: { type: "boolean" } },
          required: ["id", "on"],
        },
        run: (params, ctx) => {
          const p = params as { id: number; on: boolean };
          return ctx.invoke("set_routine_enabled", { id: p.id, enabled: p.on });
        },
      },
      {
        name: "list",
        description: "List routines with their next fire time.",
        params: { type: "object", properties: {} },
        run: (_params, ctx) => ctx.invoke("list_routines"),
      },
    ],
  });

  registerModule({
    type: "second-brain",
    title: "Second Brain",
    icon: "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor' stroke-width='1.3'><circle cx='8' cy='8' r='6.2'/><circle cx='8' cy='8' r='3'/><circle cx='8' cy='8' r='0.9' fill='currentColor'/></svg>",
    component: SecondBrain,
    settings: SecondBrainSettings,
    defaultSize: { w: 0, h: 0 },
    singleton: true,
    background: true,
    actions: [
      {
        name: "refresh",
        description: "Rebuild the workspace graph now.",
        params: { type: "object", properties: {} },
        run: (_params, ctx) => ctx.invoke("get_workspace_graph").then((g) => ({ files: (g as { files: unknown[] }).files.length })),
      },
      {
        name: "open",
        description: "Open the full-screen Second Brain, optionally focused on a workspace-relative file path.",
        params: { type: "object", properties: { path: { type: "string" } } },
        run: async (params, ctx) => {
          const path = (params as { path?: string }).path;
          ctx.emit("open-second-brain", { focus: path ? `file:${path}` : null });
          return { opened: true, focus: path ?? null };
        },
      },
      {
        name: "search",
        description: "Open the Second Brain with a search query highlighting matching notes, skills and routines.",
        params: { type: "object", properties: { q: { type: "string" } }, required: ["q"] },
        run: async (params, ctx) => {
          const q = String((params as { q: string }).q);
          ctx.emit("open-second-brain", { focus: null, query: q });
          return { opened: true, query: q };
        },
      },
    ],
  });

  registerModule({
    type: "md-file",
    title: "Markdown",
    icon: "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor' stroke-width='1.4' stroke-linejoin='round'><rect x='1.5' y='3.5' width='13' height='9' rx='1.5'/><path d='M4 10V6l2 2 2-2v4M11 6v4m-1.5-1.5L11 10l1.5-1.5'/></svg>",
    component: MdFile,
    settings: MdFileSettings,
    defaultSize: { w: 480, h: 420 },
    minSize: { w: 260, h: 160 },
    stageable: true,
    actions: [
      {
        name: "open",
        description: "Open a workspace-relative Markdown file in this instance (read mode).",
        params: { type: "object", properties: { path: { type: "string" } }, required: ["path"] },
        run: async (params, ctx) => {
          const path = String((params as { path: string }).path);
          ctx.config.update((c) => ({ ...c, path, mode: "read" }));
          return { path };
        },
      },
      {
        name: "setMode",
        description: 'Switch between "read" and "edit".',
        params: { type: "object", properties: { mode: { type: "string", enum: ["read", "edit"] } }, required: ["mode"] },
        run: async (params, ctx) => {
          const mode = (params as { mode: string }).mode === "edit" ? "edit" : "read";
          ctx.config.update((c) => ({ ...c, mode }));
          return { mode };
        },
      },
      {
        name: "getContent",
        description: "Return the file's current on-disk content.",
        params: { type: "object", properties: {} },
        run: (_params, ctx) => {
          let path = "";
          ctx.config.subscribe((c) => (path = typeof c.path === "string" ? c.path : ""))();
          return ctx.invoke("read_workspace_file", { rel: path });
        },
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

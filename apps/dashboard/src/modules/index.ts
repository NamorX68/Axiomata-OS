/**
 * Registers every built-in module. Called once from `main.ts`.
 *
 * Adding a module = a new file in this folder + one `registerModule(...)` line
 * here. The two `dummy*` entries are dev-only scaffolding (a plain one and a
 * singleton to exercise the guard).
 */

import { get } from "svelte/store";

import { registerModule } from "../core/registry";
import type { Routine, RunRecord, WorkspaceFile } from "../core/backend";
import { CALENDAR_SKILL_NAME, createCalendarEvent, deleteCalendarEvent, filterByCalendar, loadLatestCalendarDigest, parseCalendarDigest } from "../core/calendar";
import { todayIso as calendarToday, weekRange } from "../core/monthGrid";
import { loadLatestMailDigest, MAIL_SKILL_NAME, parseMailDigest } from "../core/mail";
import { completeReminderTask, createReminderTask, deleteReminderTask, loadLatestReminderDigest, parseReminderDigest, REMINDERS_SKILL_NAME, tasksForList } from "../core/reminders";
import { resolveSkillName } from "../core/skillRun";
import type { ModuleContext } from "../core/types";
import {
  addTodo,
  completeTodo,
  parseTodoDoc,
  serializeTodoDoc,
  TODO_PATH,
  todayIso,
  type TodoDoc,
} from "../core/todo";
import Calendar from "./calendar.svelte";
import CalendarSettings from "./calendar-settings.svelte";
import Dummy from "./dummy.svelte";
import DummySettings from "./dummy-settings.svelte";
import Mail from "./mail.svelte";
import MailSettings from "./mail-settings.svelte";
import MdFile from "./md-file.svelte";
import MdFileSettings from "./md-file-settings.svelte";
import MemoryStatus from "./memory-status.svelte";
import MemoryStatusSettings from "./memory-status-settings.svelte";
import Reminders from "./reminders.svelte";
import RemindersSettings from "./reminders-settings.svelte";
import RoutinesBoard from "./routines-board.svelte";
import SecondBrain from "./second-brain.svelte";
import SecondBrainSettings from "./second-brain-settings.svelte";
import RoutinesBoardSettings from "./routines-board-settings.svelte";
import SkillsDeck from "./skills-deck.svelte";
import SkillsDeckSettings from "./skills-deck-settings.svelte";
import { measureChar } from "./TerminalScreen";
import { ensureTerminalSettingsLoaded, terminalSettings } from "./terminalSettings";
import Terminal from "./terminal.svelte";
import TerminalSettings from "./terminal-settings.svelte";
import Todo from "./todo.svelte";
import TodoSettings from "./todo-settings.svelte";

const DUMMY_ICON =
  "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor'><rect x='2.5' y='2.5' width='11' height='11' rx='2'/></svg>";

/** A freshly placed Terminal tile's target size, in characters — owner
 *  request: "immer von 120x60 (Zeichen) ausgehen" (always start from
 *  120×60 characters), not a fixed pixel size that happens to fit a
 *  different number of cells depending on the configured font. */
const TERMINAL_DEFAULT_COLS = 120;
const TERMINAL_DEFAULT_ROWS = 60;
/** Rough allowance for the tile's own chrome around the canvas — the front
 *  face's header (icon + title line, its own padding) sits above the
 *  canvas, which otherwise fills the tile body exactly (`width/height:
 *  100%`, no padding of its own). Not a measured value (that would need a
 *  real mounted `Tile.svelte` instance, not just a throwaway canvas) — a
 *  deliberately generous estimate in the same spirit as this codebase's
 *  other "rough footprint, doesn't need to be exact" constants (e.g.
 *  `second-brain.svelte`'s `MENU_W`/`MENU_H`). Being a little off just
 *  means the placed tile's real row/col count (genuinely measured against
 *  the actual canvas once mounted, see `terminal.svelte`'s
 *  `measureAndSize`) ends up a line or two short of/over 120×60, not that
 *  anything breaks.
 */
const TERMINAL_CHROME_H_PX = 44;

/** Computes a Terminal tile's starting pixel size so it actually fits
 *  `TERMINAL_DEFAULT_COLS`×`TERMINAL_DEFAULT_ROWS` characters at whatever
 *  font is currently configured (`terminalSettings.fontSizePx`/
 *  `fontFamily`, Checkpoint 5b) — falling back to the theme's own
 *  `--ax-font-mono`/`--ax-font-size-sm` tokens when unset, same fallback
 *  `terminal.svelte`'s own `currentFont` already uses. Measured against a
 *  throwaway, never-attached `<canvas>` (`TerminalScreen.measureChar` only
 *  needs a 2D context, not a mounted element) rather than the real tile's
 *  own canvas, since this runs *before* any tile exists yet — it's what
 *  `core/lifecycle.ts`'s `createInstance` calls to decide a new instance's
 *  size in the first place. Reads `terminalSettings` synchronously (`get`,
 *  not `$terminalSettings` — this isn't a Svelte component).
 *
 *  `registerBuiltins` (below) kicks off `ensureTerminalSettingsLoaded()`
 *  speculatively at app boot specifically so this doesn't hit the store's
 *  empty pre-load default for the *first* Terminal tile of a session
 *  (architecture review, Checkpoint 5e: without that boot-time kick-off,
 *  this always fell back to the theme's CSS font here, silently missing
 *  a real saved custom font for exactly that one placement — a
 *  deterministic, not just theoretical, gap). By the time a user actually
 *  reaches "Add module → Terminal", that IPC round trip has very likely
 *  already resolved; if it somehow hasn't (a genuinely fast/scripted
 *  first action), this still degrades gracefully to the theme default
 *  rather than blocking or throwing. Thrown errors (`ctx === null` below)
 *  are a separate case, caught by `createInstance` itself, not here. */
function computeTerminalDefaultSize(): { w: number; h: number } {
  const settings = get(terminalSettings);
  const cs = getComputedStyle(document.documentElement);
  const fallbackFamily = cs.getPropertyValue("--ax-font-mono").trim() || "monospace";
  const fallbackSizePx = cs.getPropertyValue("--ax-font-size-sm").trim() || "13px";
  const size = typeof settings.fontSizePx === "number" ? `${settings.fontSizePx}px` : fallbackSizePx;
  const family =
    typeof settings.fontFamily === "string" && settings.fontFamily.trim() ? settings.fontFamily : fallbackFamily;

  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  // No 2D context available (shouldn't happen in a real browser/WKWebView,
  // but not impossible in an unusual embedding) — the module's own static
  // `defaultSize` below is the fallback `createInstance` uses whenever this
  // throws or is absent, so failing loudly here is fine.
  if (!ctx) throw new Error("2D canvas context unavailable");
  const metrics = measureChar(ctx, `${size} ${family}`);
  return {
    w: Math.round(TERMINAL_DEFAULT_COLS * metrics.width),
    h: Math.round(TERMINAL_DEFAULT_ROWS * metrics.height) + TERMINAL_CHROME_H_PX,
  };
}

export function registerBuiltins(): void {
  registerModule({
    type: "memory-status",
    title: "Memory",
    icon: "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor' stroke-width='1.4' stroke-linejoin='round'><path d='M3 2.5h7l3 3v8H3z'/><path d='M10 2.5v3h3M5.5 8.5h5M5.5 11h5'/></svg>",
    component: MemoryStatus,
    settings: MemoryStatusSettings,
    defaultSize: { w: 360, h: 150 },
    minSize: { w: 240, h: 90 },
    // One workspace → one router status. No reason for a second tile.
    singleton: true,
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
    // Shows every discovered skill — one deck is the whole set.
    singleton: true,
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
    // Lists every routine — one board is the whole schedule.
    singleton: true,
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
        name: "edit",
        description:
          "Replace a routine's name/cron/target/backend by id — a full replace, not a partial patch: pass every field again, not just the one that changed. Its enabled state is left untouched.",
        params: {
          type: "object",
          properties: {
            id: { type: "integer" },
            name: { type: "string" },
            cron: { type: "string" },
            skill: { type: "string" },
            prompt: { type: "string" },
          },
          required: ["id", "name", "cron"],
        },
        run: async (params, ctx) => {
          const p = params as { id: number; name: string; cron: string; skill?: string; prompt?: string };
          const target = p.skill ? { type: "skill", value: p.skill } : { type: "prompt", value: p.prompt ?? "" };
          // `update_routine` is a full replace (see its own doc comment), so
          // the current `enabled` has to be read back first — otherwise a
          // disabled routine would silently come back on with every edit.
          const current = (await ctx.invoke<Routine[]>("list_routines")).find((r) => r.id === p.id);
          return ctx.invoke("update_routine", {
            id: p.id,
            new: { name: p.name, cron_expr: p.cron, target, backend: null, enabled: current?.enabled ?? true },
          });
        },
      },
      {
        name: "delete",
        description: "Permanently delete a routine and its firing history by id.",
        params: {
          type: "object",
          properties: { id: { type: "integer" } },
          required: ["id"],
        },
        run: (params, ctx) => {
          const p = params as { id: number };
          return ctx.invoke("delete_routine", { id: p.id });
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
    title: "Document",
    icon: "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor' stroke-width='1.4' stroke-linejoin='round'><rect x='1.5' y='3.5' width='13' height='9' rx='1.5'/><path d='M4 10V6l2 2 2-2v4M11 6v4m-1.5-1.5L11 10l1.5-1.5'/></svg>",
    component: MdFile,
    settings: MdFileSettings,
    defaultSize: { w: 480, h: 420 },
    minSize: { w: 260, h: 160 },
    stageable: true,
    actions: [
      {
        name: "open",
        description: "Open a workspace-relative file in this instance (read mode) — Markdown, HTML, any UTF-8 text file, or an image.",
        params: { type: "object", properties: { path: { type: "string" } }, required: ["path"] },
        run: async (params, ctx) => {
          const path = String((params as { path: string }).path);
          ctx.config.update((c) => ({ ...c, path, mode: "read" }));
          return { path };
        },
      },
      {
        name: "setMode",
        description: 'Switch between "read" and "edit" (edit works for Markdown, HTML and any text file — not images).',
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

  registerModule({
    type: "todo",
    title: "ToDo",
    icon: "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor' stroke-width='1.4' stroke-linecap='round' stroke-linejoin='round'><rect x='2' y='2.5' width='11' height='11' rx='2'/><path d='m5 8 2.2 2.2L11 6'/></svg>",
    component: Todo,
    settings: TodoSettings,
    defaultSize: { w: 340, h: 360 },
    minSize: { w: 240, h: 140 },
    singleton: true,
    actions: (() => {
      // The actions work statelessly on ToDo.md itself (not the mounted
      // tile's state), so the agent bridge and `/todo` still work when no
      // instance is placed. `loadDoc` treats a missing file as an empty list.
      const loadDoc = async (ctx: ModuleContext): Promise<{ doc: TodoDoc; raw: string | null }> => {
        try {
          const file = await ctx.invoke<WorkspaceFile>("read_workspace_file", { rel: TODO_PATH });
          return { doc: parseTodoDoc(file.content), raw: file.content };
        } catch (err) {
          if (/no such file|not found|does not exist/i.test(String(err))) {
            return { doc: { open: [], done: [] }, raw: null };
          }
          throw err;
        }
      };
      const saveDoc = async (ctx: ModuleContext, next: TodoDoc, raw: string | null): Promise<void> => {
        const serialized = serializeTodoDoc(next);
        if (serialized !== raw) await ctx.invoke("write_workspace_file", { rel: TODO_PATH, content: serialized });
      };
      return [
        {
          name: "add",
          description: "Append an open task to ToDo.md.",
          params: { type: "object", properties: { text: { type: "string" } }, required: ["text"] },
          run: async (params, ctx) => {
            const text = String((params as { text: string }).text);
            const { doc, raw } = await loadDoc(ctx);
            const next = addTodo(doc, text);
            await saveDoc(ctx, next, raw);
            return { open: next.open.length };
          },
        },
        {
          name: "complete",
          description: "Mark the first open task whose text contains `text` as done (moved to the Done section, dated today).",
          params: { type: "object", properties: { text: { type: "string" } }, required: ["text"] },
          run: async (params, ctx) => {
            const needle = String((params as { text: string }).text).trim().toLowerCase();
            const { doc, raw } = await loadDoc(ctx);
            const index = doc.open.findIndex((it) => it.text.toLowerCase().includes(needle));
            if (index === -1) return { completed: false };
            const next = completeTodo(doc, index, todayIso());
            await saveDoc(ctx, next, raw);
            return { completed: true, text: doc.open[index].text };
          },
        },
        {
          name: "list",
          description: "Return the open and done tasks from ToDo.md.",
          params: { type: "object", properties: {} },
          run: async (_params, ctx) => {
            const { doc } = await loadDoc(ctx);
            return {
              open: doc.open.map((it) => it.text),
              done: doc.done.map((it) => ({ text: it.text, doneOn: it.doneOn })),
            };
          },
        },
      ];
    })(),
  });

  registerModule({
    type: "calendar",
    title: "Calendar",
    icon: "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor' stroke-width='1.4' stroke-linecap='round' stroke-linejoin='round'><rect x='2' y='3' width='12' height='11' rx='1.5'/><path d='M2 6.5h12M5 1.5v3M11 1.5v3'/></svg>",
    component: Calendar,
    settings: CalendarSettings,
    // Tall enough for the mini-month + a few agenda days without scrolling.
    defaultSize: { w: 380, h: 540 },
    minSize: { w: 300, h: 420 },
    singleton: true,
    actions: [
      {
        name: "refresh",
        description: "Runs the calendar-digest skill now and returns a summary of what it found.",
        params: { type: "object", properties: {} },
        run: async (_params, ctx) => {
          const run = await ctx.invoke<RunRecord>("run_skill", { name: resolveSkillName(get(ctx.config), CALENDAR_SKILL_NAME) });
          if (run.status === "failed") return { ok: false, error: run.error ?? "run failed" };
          const digest = parseCalendarDigest(run.stdout);
          return { ok: true, calendars: digest.calendars, events: digest.events.length };
        },
      },
      {
        name: "list",
        description:
          "Returns events from the last calendar-digest run (does not trigger a new run). Optionally filtered to one `calendar`, and/or to a window of `days` (default 7) starting at `from` (YYYY-MM-DD, default today) — the same 7-day slice the tile's agenda shows.",
        params: {
          type: "object",
          properties: {
            calendar: { type: "string" },
            from: { type: "string", description: "YYYY-MM-DD; defaults to today" },
            days: { type: "integer", description: "window length in days; defaults to 7" },
          },
        },
        run: async (params, ctx) => {
          const result = await loadLatestCalendarDigest(ctx.invoke, resolveSkillName(get(ctx.config), CALENDAR_SKILL_NAME));
          if (!result.run) return { events: [], note: "no run yet" };
          if (result.error) return { events: [], error: result.error };
          const p = params as { calendar?: unknown; from?: unknown; days?: unknown };
          const calendar = typeof p.calendar === "string" ? p.calendar || null : null;
          let events = filterByCalendar(result.digest.events, calendar);
          if (typeof p.from === "string" || typeof p.days === "number") {
            const from = typeof p.from === "string" && p.from ? p.from : calendarToday();
            const len = typeof p.days === "number" && p.days > 0 ? Math.floor(p.days) : 7;
            const window = new Set(weekRange(from, len));
            events = events.filter((e) => window.has(e.start.slice(0, 10)));
          }
          return { events };
        },
      },
      {
        name: "create",
        description: "Creates a calendar event via a one-shot instruct turn and returns it. All-day when startTime/endTime are omitted.",
        params: {
          type: "object",
          properties: {
            title: { type: "string" },
            calendar: { type: "string" },
            date: { type: "string", description: "YYYY-MM-DD" },
            startTime: { type: "string", description: "HH:mm, omit for an all-day event" },
            endTime: { type: "string", description: "HH:mm, defaults to startTime" },
            location: { type: "string" },
          },
          required: ["title", "calendar", "date"],
        },
        run: async (params, ctx) => {
          const p = params as { title: string; calendar: string; date: string; startTime?: string; endTime?: string; location?: string };
          const event = await createCalendarEvent(ctx.invoke, {
            title: p.title,
            calendar: p.calendar,
            date: p.date,
            startTime: p.startTime ?? null,
            endTime: p.endTime ?? null,
            location: p.location ?? null,
          });
          return { event };
        },
      },
      {
        name: "delete",
        description: "Deletes a calendar event by id (from the last calendar-digest run's events) via a one-shot instruct turn.",
        params: { type: "object", properties: { id: { type: "string" } }, required: ["id"] },
        run: async (params, ctx) => {
          const id = (params as { id?: unknown }).id;
          if (typeof id !== "string" || !id) return { ok: false, error: 'missing required "id" param' };
          await deleteCalendarEvent(ctx.invoke, id);
          return { ok: true };
        },
      },
    ],
  });

  registerModule({
    type: "reminders",
    title: "Reminders",
    icon: "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor' stroke-width='1.4' stroke-linecap='round' stroke-linejoin='round'><rect x='2.5' y='2' width='11' height='12' rx='1.5'/><path d='M5.5 5.5h5M5.5 8h5M5.5 10.5h3'/></svg>",
    component: Reminders,
    settings: RemindersSettings,
    defaultSize: { w: 340, h: 360 },
    minSize: { w: 240, h: 160 },
    singleton: true,
    actions: [
      {
        name: "refresh",
        description: "Runs the reminders-digest skill now and returns a summary of what it found.",
        params: { type: "object", properties: {} },
        run: async (_params, ctx) => {
          const run = await ctx.invoke<RunRecord>("run_skill", { name: resolveSkillName(get(ctx.config), REMINDERS_SKILL_NAME) });
          if (run.status === "failed") return { ok: false, error: run.error ?? "run failed" };
          const digest = parseReminderDigest(run.stdout);
          return { ok: true, lists: digest.lists, tasks: digest.tasks.length };
        },
      },
      {
        name: "lists",
        description:
          'Returns the reminder list names from the last reminders-digest run (does not trigger a new run). There is no "all lists" task view — call this first, then "list" with one of these names.',
        params: { type: "object", properties: {} },
        run: async (_params, ctx) => {
          const result = await loadLatestReminderDigest(ctx.invoke, resolveSkillName(get(ctx.config), REMINDERS_SKILL_NAME));
          if (result.error) return { lists: [], error: result.error };
          return { lists: result.digest.lists };
        },
      },
      {
        name: "list",
        description: 'Returns the open tasks on one reminder list (an exact name from the "lists" action) from the last reminders-digest run — does not trigger a new run.',
        params: { type: "object", properties: { list: { type: "string" } }, required: ["list"] },
        run: async (params, ctx) => {
          const list = typeof (params as { list?: unknown }).list === "string" ? (params as { list: string }).list : "";
          if (!list) return { tasks: [], error: 'missing required "list" param — call the "lists" action first' };
          const result = await loadLatestReminderDigest(ctx.invoke, resolveSkillName(get(ctx.config), REMINDERS_SKILL_NAME));
          if (result.error) return { tasks: [], error: result.error };
          return { tasks: tasksForList(result.digest.tasks, list) };
        },
      },
      {
        name: "create",
        description: "Creates a reminder via a one-shot instruct turn and returns it, priority \"none\".",
        params: {
          type: "object",
          properties: {
            title: { type: "string" },
            list: { type: "string" },
            dueDate: { type: "string", description: "YYYY-MM-DD or YYYY-MM-DD HH:mm:ss, omit for none" },
            notes: { type: "string" },
          },
          required: ["title", "list"],
        },
        run: async (params, ctx) => {
          const p = params as { title: string; list: string; dueDate?: string; notes?: string };
          const task = await createReminderTask(ctx.invoke, { title: p.title, list: p.list, dueDate: p.dueDate ?? null, notes: p.notes ?? null });
          return { task };
        },
      },
      {
        name: "complete",
        description: "Marks a reminder complete by id (from the last reminders-digest run's tasks) via a one-shot instruct turn.",
        params: { type: "object", properties: { id: { type: "string" } }, required: ["id"] },
        run: async (params, ctx) => {
          const id = (params as { id?: unknown }).id;
          if (typeof id !== "string" || !id) return { ok: false, error: 'missing required "id" param' };
          await completeReminderTask(ctx.invoke, id);
          return { ok: true };
        },
      },
      {
        name: "delete",
        description: "Deletes a reminder by id (from the last reminders-digest run's tasks) via a one-shot instruct turn.",
        params: { type: "object", properties: { id: { type: "string" } }, required: ["id"] },
        run: async (params, ctx) => {
          const id = (params as { id?: unknown }).id;
          if (typeof id !== "string" || !id) return { ok: false, error: 'missing required "id" param' };
          await deleteReminderTask(ctx.invoke, id);
          return { ok: true };
        },
      },
    ],
  });

  registerModule({
    type: "mail",
    title: "Mail",
    icon: "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor' stroke-width='1.4' stroke-linecap='round' stroke-linejoin='round'><rect x='1.5' y='3' width='13' height='10' rx='1.5'/><path d='M2 4.5l6 4.5 6-4.5'/></svg>",
    component: Mail,
    settings: MailSettings,
    defaultSize: { w: 340, h: 360 },
    minSize: { w: 240, h: 160 },
    singleton: true,
    actions: [
      {
        name: "refresh",
        description: "Runs the mail-digest skill now and returns a summary of what it found.",
        params: { type: "object", properties: {} },
        run: async (_params, ctx) => {
          const run = await ctx.invoke<RunRecord>("run_skill", { name: resolveSkillName(get(ctx.config), MAIL_SKILL_NAME) });
          if (run.status === "failed") return { ok: false, error: run.error ?? "run failed" };
          const digest = parseMailDigest(run.stdout);
          return { ok: true, emails: digest.emails.length };
        },
      },
      {
        name: "list",
        description: "Returns the curated emails from the last mail-digest run (does not trigger a new run).",
        params: { type: "object", properties: {} },
        run: async (_params, ctx) => {
          const result = await loadLatestMailDigest(ctx.invoke, resolveSkillName(get(ctx.config), MAIL_SKILL_NAME));
          if (result.error) return { emails: [], error: result.error };
          return { emails: result.digest.emails };
        },
      },
    ],
  });

  registerModule({
    type: "terminal",
    title: "Terminal",
    icon: "<svg viewBox='0 0 16 16' fill='none' stroke='currentColor' stroke-width='1.4' stroke-linejoin='round'><rect x='1.5' y='2.5' width='13' height='11' rx='1.5'/><path d='M4 6.5 6.5 9 4 11.5M8 11.5h4'/></svg>",
    component: Terminal,
    settings: TerminalSettings,
    // Static fallback for `computeDefaultSize`'s own failure path, and for
    // anything that reads `defaultSize` directly rather than going through
    // `createInstance` (e.g. `ModulePicker`'s size label) — see
    // `core/types.ts`'s own doc comment on `computeDefaultSize`.
    defaultSize: { w: 640, h: 400 },
    computeDefaultSize: computeTerminalDefaultSize,
    // The real computed size (font-dependent) can land far enough from the
    // static `defaultSize` above (roughly 2× in both dimensions at a
    // typical font) that showing it as a pixel figure would read as
    // actively wrong, not just approximate — architecture review,
    // Checkpoint 5e. A font-independent, honestly-approximate label
    // instead.
    sizeLabel: `~${TERMINAL_DEFAULT_COLS}×${TERMINAL_DEFAULT_ROWS} chars`,
    minSize: { w: 320, h: 200 },
    // One shell process per placed tile (Checkpoint 0/1 of
    // docs/plans/terminal.md) — the dashboard's existing multi-instance
    // mechanism is the "multiple terminals" story, not a tab bar inside the
    // module itself.
    singleton: false,
  });
  // Speculative, fire-and-forget: kicks off `terminalSettings`'s load right
  // at app boot instead of waiting for the first Terminal instance to
  // mount (`ensureTerminalSettingsLoaded`'s normal trigger). Architecture
  // review, Checkpoint 5e: without this, `computeTerminalDefaultSize`
  // above — called synchronously by `createInstance` the moment the
  // *first* Terminal tile of a session is placed, which can easily happen
  // before any Terminal component has ever mounted — always saw the
  // store's empty pre-load `{}`, silently sizing that one tile off the
  // theme's CSS font defaults instead of the owner's real saved
  // `fontSizePx`/`fontFamily`, even when they'd already set one. Cheap and
  // idempotent (`ensureTerminalSettingsLoaded`'s own doc comment) — by the
  // time a user actually clicks "Add module → Terminal", this IPC round
  // trip has very likely already resolved.
  void ensureTerminalSettingsLoaded();

  if (import.meta.env.DEV) {
    registerModule({
      type: "dummy",
      title: "Dummy",
      icon: DUMMY_ICON,
      component: Dummy,
      settings: DummySettings,
      defaultSize: { w: 260, h: 160 },
      minSize: { w: 160, h: 100 },
      dev: true,
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
      dev: true,
    });
  }
}

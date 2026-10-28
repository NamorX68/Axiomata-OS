# A Rust GPU frontend — stack decision and migration plan

Status: **stack decided and validated by prototype** (2026-10-16); migration not
started yet — current real work is the M7 harness, the migration follows.
Prototype: `~/.axiomata/prototypes/hexball` (disposable, never in this repo).

## 1. The decision: vello + wgpu

The Tauri/Svelte frontend is replaced, gradually, by a pure-Rust GPU frontend:

- **vello 0.11** — GPU vector renderer; the direct replacement for the
  dashboard's Canvas-2D usage (graph ball, hex maps, editor surface, terminal)
- **wgpu** — Metal on macOS, **Vulkan on Linux**, DX12 on Windows underneath;
  the Linux path comes free with this choice
- **winit 0.30** — windowing and input
- **kurbo / peniko** — geometry and colour/brush types (vello companions)
- **parley** — font layout for all text (vello 0.11 moved text out of peniko;
  labels, search HUD, editor, terminal all go through it)

**Rejected: Bevy.** Too heavy as an app framework — its own app loop fights a
custom one, UI tooling is immature, and its release churn would dominate a
long-lived codebase. vello is a rendering library, not a framework: we keep
full control of the loop, and the scene-graph model maps 1:1 onto what the
current Canvas code does.

### What the prototype proved (`hexball`, ~900 lines, one window)

- A 157-tile hexagon ball (the vault's real file count) clad on a sphere,
  cluster Voronoi colours, draped corners — at interactive frame rates
- The visual language of the current dashboard is reproducible: dark dormant
  dome with age-driven brightness (new files burn bright), metallic sheen
  (per-tile gradient following each tile's own rotation), hue drift, glow on
  the newest tiles, ball self-yaw + tilt nod (parameters taken from the
  current `render.ts`: `spin × 3 = 0.06 rad/s`, nod `sin(t·0.15)·0.08`)
- The ball ↔ flat-honeycomb fly-through works (camera swings face-on entering
  the map, restores the orbit leaving); search-by-name with fly-to-matches
- Stack API facts are learned and pinned: vello 0.11 render path
  (`render_to_texture` → blitter → `queue.present`), `context.resize_surface`,
  peniko 0.6 gradients (`Gradient::new_linear` / `new_radial`, stop tuples),
  kurbo 0.13 (`BezPath::close_path`, `Stroke` in kurbo not peniko)

**Not yet proven by the prototype:** general UI chrome (panels, lists, dialogs,
settings) — vello renders scenes, not widgets. See open question §7.1.

## 2. Rule: harness surfaces are Rust from now on

The harness (agent) work now under way (M7: `axiomata-ide`, agent lifecycle,
git layer) must **not** grow new Svelte surfaces. Anything the harness shows
(agent status, plans, diffs, PTY tiles) is built on the Rust side (Tauri
commands today, native code tomorrow) so the migration removes code instead of
chasing new TS debt. The editor's own engine already follows this rule (the
TS engine in `src/editor/` is written to be extractable/replaceable per D1).

## 3. Why migrate

- **One language.** ~47k lines Rust (crates + Tauri shell) vs ~79k TS/Svelte;
  the Rust side already owns all the load-bearing engines (core, files, LSP,
  terminal PTY+VT, git, opencode bridge).
- **The webview is the ceiling.** Canvas-2D graph rendering, the terminal
  renderer (~3k lines of xterm-style glue), and the editor surface all fight
  the browser. GPU rendering removes the bridge (no `invoke` round-trips per
  frame), and the editor/terminal get sub-frame latency.
- **Linux / Axiomata-OS ambition.** wgpu/Vulkan makes the frontend portable;
  the platform-specific bits are already isolated (`axiomata-macos`,
  Apple-connector skills run through opencode's MCP). A long-term own-OS
  ambition is a direction, not a commitment (§6).

Estimated cumulative effort for the full migration: **6–12 months**, done in
phases next to feature work, never as a big bang.

## 4. Current state (baseline for the phases)

- Rust engines: `axiomata-core` (DB, skills, routines, board), `axiomata-files`
  (guarded file service + LSP + search), `axiomata-terminal` (PTY + VT screen),
  `axiomata-macos`, `axiomata-opencode` — all reusable as-is; the frontend
  migration does not touch them.
- TS/Svelte to replace, by size: `src/editor/` (the editor engine — rope, Vi
  machine, tree-sitter glue, LSP client, ~largest single block), `src/fileapp/`,
  `src/graph/` (ball renderer + layouts), the module panels, the shell.
- The terminal is the smallest complete surface: engine in Rust, renderer is
  the only TS left.

## 5. Migration phases (sketch)

Each phase ships something usable on its own; TS dies surface by surface.
Pilot surfaces run as **standalone winit windows next to the Tauri app**
(the "extractable app" property `axiomata-terminal` was designed for), so no
phase ever blocks the running app.

- **P0 — stack validation.** ✅ Done (`hexball`).
- **P1 — terminal pilot (~1–2 weeks).** Standalone winit window: PTY tiles
  from `axiomata-terminal` rendered via vello + parley (glyph cache, cursor,
  selection), full keyboard/IME input. Validates the text-heavy path, which
  the ball prototype deliberately skipped.
- **P2 — Second Brain graph.** The hexball code fed by the real graph model
  (`graph/layout.ts` logic ported; data via the existing core commands).
  Search-driven navigation to be tried here (§7.2).
- **P3 — widget/chrome strategy** (decision gate, §7.1), then module panels
  (ToDo, digests — mostly text lists, they map cleanly onto vello + parley).
- **P4 — editor surface.** Port the TS editor engine to Rust on top of
  `axiomata-files` (rope, Vi, tree-sitter WASM→native, LSP glue). Largest
  single phase; the engine's app-free design (D1) is what makes it feasible.
- **P5 — shell.** winit window replaces Tauri: menus (`axiomata-macos`),
  tray, dialogs (rfd), staging/panels, single window with views. Tauri is
  dropped when the last Svelte surface is gone.

## 6. Linux / OS ambition

wgpu/Vulkan gives the renderer portability for free; the real Linux work is
platform crates, not rendering: `axiomata-macos` (clipboard, fonts, display
density) needs trait-shaped fallbacks, Apple-only connector skills simply
don't exist there (Reminders/Calendar/Mail are MCP skills — they vanish, the
app still runs), dialogs via rfd, notifications/autostart per-OS. The
"Axiomata-OS" direction (own hardware, own compositor-ish shell someday) is
recorded here as ambition; each phase above is chosen so it stays useful
regardless of whether that ambition ever happens.

## 7. Open questions

1. **Chrome/widgets.** vello draws scenes, not widgets. Options for panels,
   dialogs, settings: (a) xilem (Linebender, vello-native, still young),
   (b) a small custom retained layer (our scene graph + parley layout +
   AccessKit), (c) keep the webview for chrome much longer (hybrid window
   management). Decide before P3; the prototype says nothing about this yet.
2. **Search-driven navigation.** The owner's idea to try in P2: search depth
   replaces the 2D map — "je genauer gesucht, desto tiefer in den Ball
   eindringen" (search a cluster → fly into it; search a file → open it).
   May make the flat honeycomb map unnecessary.
3. **Accessibility.** The webview gave us screen readers for free; a vello
   UI has none. AccessKit integration is the plan — cost unknown, decide in P1.
4. **Theming.** The `--ax-*` token set (`themes/tokens.css`) becomes a typed
   token struct in Rust; the validated `~/.axiomata/theme.css` parser stays.
5. **Text pinning.** parley must be kept version-aligned with vello/peniko
   (they release in lockstep); pin in the workspace `Cargo.toml`.
6. **Multi-window.** One winit window with internal views vs several windows
   (brain, terminal, editor) — current Tauri app is one window; keep it that
   way until a surface demands its own window.

## 8. Non-goals

- No rewrite of the Rust engines — they are the stable ground the migration
  stands on.
- No big-bang cutover; the Tauri app keeps working until P5.
- No prototype code ever enters this repo (the `~/.axiomata/prototypes/`
  rule stays in force).

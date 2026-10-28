# hexball — GPU prototype of the Second Brain ball (vello)

Stack-validation prototype for the planned Tauri/Svelte → pure-Rust migration:
can the Second Brain's hexagon ball (and later the terminal) be rendered in
Rust on the GPU directly? Answer after this demo: **yes**.

Pure demo — no Axiomata crates involved, lives only under `~/.axiomata/prototypes/`,
never touches the repo, fully deletable (`rm -rf`).

## Stack

- **vello 0.11** — GPU vector renderer (the future Canvas-2D replacement)
- **wgpu** — Metal/Vulkan/DX12 backend underneath (Linux path comes free)
- **winit 0.30** — windowing + input
- **kurbo / peniko** — geometry, colours, gradients (vello's companion crates)
- **parley 0.6** — font layout for all text (vello 0.11 moved text out of
  peniko): parley shapes and positions, vello draws via `Scene::draw_glyphs`;
  both share `linebender_resource_handle::FontData`, so a run's font passes
  straight through. Trap: `build()` alone produces no lines — the explicit
  `layout.break_all_lines(None)` pass is mandatory or the layout is empty.

## What it validates (all working)

- 157 hexagon tiles (the vault's real file count) clad on a sphere (fibonacci
  distribution, cluster Voronoi colours, draped corners → clean silhouette)
- Cluster = hue, file age = brightness (new files burn bright, old ones sink
  into the dome), subtle glow on the ~30 % newest tiles
- Metallic sheen: per-tile linear gradient whose axis follows the tile's own
  slow self-rotation; small view-driven hue drift ("Farbwechsel")
- Ball self-motion: constant yaw (0.06 rad/s, taken from the current app's
  `spin × 3`) + slow tilt nod; pauses while a tile is focused
- Click = fly-through between the ball and a flat honeycomb map (camera swings
  face-on entering the map, restores the orbit leaving it; patches sized by
  real cluster counts)
- Type-to-search by cluster name: matches light amber, Enter flies to them,
  Escape clears; map-mode Enter pans onto the patch instead
- Text everywhere, laid out by parley and drawn as vello glyph runs: the
  search field (query + blinking caret + match count + hints), title header
  (`Second Brain`, file/area counts), view word (Kugel/Karte), cluster
  labels on the flat map patches, and a focus chip naming the focused tile
  (file name · cluster · age band). `HEXBALL_TEXT_DEBUG=1` logs runs,
  glyphs, font bytes and size per laid-out line.
- Orbit camera with damping; elevation clamped to ±60° so horizontal dragging
  never degenerates into screen-plane (clock-hand) rotation

## Controls

- drag: orbit (ball) / pan (map) — wheel: zoom · click: toggle ball↔map
- double-click: focus a tile · type: search · Enter: fly to matches · Esc: clear/back

## Known gaps (deliberate, next steps)

- No App Ring (icon ring around the ball) and no honeycomb background texture
- Hit-testing is centre-distance based, no per-pixel path test
- No interactive text yet (editing, cursor, selection, IME) — the P1
  terminal pilot in `docs/plans/rust-frontend.md` covers that path

## Run

```sh
cargo run --release
```

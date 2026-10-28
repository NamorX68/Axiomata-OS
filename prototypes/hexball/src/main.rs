//! Hexball prototype: a GPU (vello) take on Axiomata's Second Brain graph.
//!
//! Pure demo, no Axiomata crates involved. 160 hexagon tiles clad a sphere
//! (fibonacci distribution, colours clustered by nearest of 8 cluster centres
//! — contiguous regions like the current canvas rendering); a click flies the
//! camera through the ball into a flat honeycomb map. Typing filters by
//! cluster name, Enter flies to the matches, Escape flies back.
//! This validates the vello + wgpu stack for both the future Second Brain
//! and the terminal renderer.

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use parley::{Alignment, AlignmentOptions, FontContext, LayoutContext, PositionedLayoutItem, StyleProperty};
use vello::kurbo::{Affine, BezPath, Circle, Point, Rect, RoundedRect, Stroke};
use vello::peniko::{Brush, Color, Fill, Gradient};
use vello::util::{RenderContext, RenderSurface};
use vello::{AaConfig, Glyph, Renderer, RendererOptions, Scene};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

/// The eight clusters — names double as the search keys (type "mail" to light
/// up the whole mail region on the ball).
const CLUSTER_NAMES: [&str; 8] = ["archiv", "studio", "kanban", "brain", "note", "mail", "skills", "routinen"];

/// Dark, earthen cluster palette in the spirit of the current canvas
/// rendering: indigo, amber, forest, stone, bone, moss, violet, slate.
const CLUSTER_COLORS: [[u8; 4]; 8] = [
    [105, 90, 190, 255],   // indigo
    [178, 120, 62, 255],   // amber
    [104, 165, 80, 255],   // forest
    [148, 142, 136, 255],  // stone
    [206, 200, 194, 255],  // bone
    [92, 128, 88, 255],    // moss
    [138, 96, 168, 255],   // violet
    [86, 100, 132, 255],   // slate
];

const BG: Color = Color::from_rgba8(7, 8, 10, 255);
/// One tile per vault file — the original dashboard shows "157 files".
const TILE_COUNT: usize = 157;

/// Plausible file names per cluster — the prototype's stand-in for the real
/// vault's 157 files, so a focused tile can name itself.
const FILE_NAMES: [&[&str]; 8] = [
    &["lesezeichen-alt.md", "rechnungen-2023.md", "tagebuch-2021.md", "projekt-x-akten.md"],
    &["mockup-hauptseite.fig", "farbschema.md", "pitch-entwurf.md", "tonspur-notiz.md"],
    &["sprint-41.md", "backlog-import.md", "review-routine.md", "blocker-heute.md"],
    &["zweite-hirn-struktur.md", "konnektome.md", "ideen-kern.md", "axiom-werkzeug.md"],
    &["inbox-ideen.md", "treff-notiz.md", "wochen-reflexion.md", "buessel-draft.md"],
    &["posteingang-themen.md", "rc-reply-draft.md", "verteiler.md", "follow-ups.md"],
    &["SKILL.md", "pdf-zusammenfassen.md", "inbox-sort.md", "mail-digest.md"],
    &["morgen-ritual.cron", "abend-review.cron", "wochen-sync.cron", "monats-routine.cron"],
];
/// World radius of the ball; tiles sit at ~0.13 of it, the original's ratio.
const BALL_R: f64 = 1.55;

/// Multiply a base colour by a brightness factor (headlight shading).
fn shade(base: [u8; 4], brightness: f64, alpha: f64) -> Color {
    let b = brightness.clamp(0.0, 1.4);
    Color::from_rgba8(
        ((base[0] as f64) * b).min(255.0) as u8,
        ((base[1] as f64) * b).min(255.0) as u8,
        ((base[2] as f64) * b).min(255.0) as u8,
        ((base[3] as f64) * alpha.clamp(0.0, 1.0)) as u8,
    )
}

/// Blend a colour towards another (straight RGB lerp).
fn mix(a: [u8; 4], b: [u8; 4], k: f64) -> [u8; 4] {
    let k = k.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * k) as u8;
    [ch(a[0], b[0]), ch(a[1], b[1]), ch(a[2], b[2]), ch(a[3], b[3])]
}

/// Blend a colour towards white — the specular stop of the metallic sheen.
fn mix_white(base: [u8; 4], k: f64) -> [u8; 4] {
    let k = k.clamp(0.0, 1.0);
    let ch = |c: u8| (c as f64 + (255.0 - c as f64) * k) as u8;
    [ch(base[0]), ch(base[1]), ch(base[2]), base[3]]
}

/// Rotate a colour's hue (the CSS hue-rotate matrix, YIQ space) — the
/// "Farbwechsel" shimmer as tiles turn towards or away from the light.
fn hue_rotate(base: [u8; 4], a: f64) -> [u8; 4] {
    let (sa, ca) = a.sin_cos();
    let (r, g, b) = ((base[0] as f64) / 255.0, (base[1] as f64) / 255.0, (base[2] as f64) / 255.0);
    let rr = 0.213 + ca * 0.787 - sa * 0.213;
    let rg = 0.715 - ca * 0.715 - sa * 0.715;
    let rb = 0.072 - ca * 0.072 + sa * 0.928;
    let gr = 0.213 - ca * 0.213 + sa * 0.143;
    let gg = 0.715 + ca * 0.285 + sa * 0.140;
    let gb = 0.072 - ca * 0.072 - sa * 0.283;
    let br = 0.213 - ca * 0.213 - sa * 0.787;
    let bg = 0.715 - ca * 0.715 + sa * 0.715;
    let bb = 0.072 + ca * 0.928 + sa * 0.072;
    let ch = |c: f64| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    [
        ch(rr * r + rg * g + rb * b),
        ch(gr * r + gg * g + gb * b),
        ch(br * r + bg * g + bb * b),
        base[3],
    ]
}

// ---------------------------------------------------------------------- text

/// Text is a two-crate affair since vello 0.11 moved font layout out of
/// peniko: parley shapes and positions, vello draws the glyphs. Both sides
/// share `linebender_resource_handle::FontData`, so a run's font passes
/// straight from `gr.run().font()` into `Scene::draw_glyphs`.
struct TextKit {
    fonts: FontContext,
    layout: LayoutContext,
    /// device pixel ratio — the scene works in physical px, text is sized in
    /// logical px like the rest of the UI would be
    scale: f64,
}

/// Horizontal anchor of a text block around the given point.
#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Center,
    Right,
}

impl TextKit {
    fn new(scale: f64) -> Self {
        let mut kit = TextKit {
            fonts: FontContext::new(),
            layout: LayoutContext::new(),
            scale,
        };
        if std::env::var_os("HEXBALL_TEXT_DEBUG").is_some() {
            let count = kit.fonts.collection.family_names().count();
            let names: Vec<String> =
                kit.fonts.collection.family_names().take(8).map(String::from).collect();
            let generics = kit
                .fonts
                .collection
                .generic_families(parley::fontique::GenericFamily::SansSerif)
                .count();
            eprintln!("dbg families={count} first={names:?} sans_serif_ids={generics}");
        }
        kit
    }

    /// Lay out a single line at `size` logical px. Glyph positions inside a
    /// layout are y-down from the block's top-left — vello's convention too.
    fn layout_line(&mut self, text: &str, size: f64) -> parley::Layout<[u8; 4]> {
        let px = (size * self.scale) as f32;
        let mut builder = self.layout.ranged_builder(&mut self.fonts, text, 1.0, false);
        builder.push_default(StyleProperty::FontSize(px));
        let mut layout = builder.build(text);
        // `build` only shapes the runs — without an explicit line-breaking
        // pass the layout has no lines at all (zero width, zero glyphs),
        // which cost a whole debugging session to find
        layout.break_all_lines(None);
        layout.align(None, Alignment::Start, AlignmentOptions::default());
        if std::env::var_os("HEXBALL_TEXT_DEBUG").is_some() {
            let mut runs = 0usize;
            let mut glyphs = 0usize;
            let mut font_len = 0usize;
            let mut fsize = 0.0f32;
            for line in layout.lines() {
                for item in line.items() {
                    if let PositionedLayoutItem::GlyphRun(gr) = item {
                        runs += 1;
                        glyphs += gr.positioned_glyphs().count();
                        font_len = gr.run().font().data.as_ref().len();
                        fsize = gr.run().font_size();
                    }
                }
            }
            eprintln!(
                "dbg text={text:?} px={px} w={:.1} h={:.1} runs={runs} glyphs={glyphs} font_len={font_len} fsize={fsize}",
                layout.width(),
                layout.height()
            );
        }
        layout
    }

    /// Draw one line with its top-left anchored at `at` (shifted horizontally
    /// by `align`). Returns the block's size in physical px.
    fn draw(
        &mut self,
        scene: &mut Scene,
        text: &str,
        size: f64,
        at: (f64, f64),
        color: [u8; 4],
        align: Align,
    ) -> (f64, f64) {
        let layout = self.layout_line(text, size);
        let (bw, bh) = (layout.width() as f64, layout.height() as f64);
        let x0 = at.0
            - match align {
                Align::Left => 0.0,
                Align::Center => bw / 2.0,
                Align::Right => bw,
            };
        let solid = Color::from_rgba8(color[0], color[1], color[2], color[3]);
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(gr) = item else { continue };
                let run = gr.run();
                scene.draw_glyphs(run.font())
                    .transform(Affine::translate((x0, at.1)))
                    .font_size(run.font_size())
                    .normalized_coords(run.normalized_coords())
                    .brush(solid)
                    .draw(
                        Fill::NonZero,
                        gr.positioned_glyphs().map(|g| Glyph { id: g.id, x: g.x, y: g.y }),
                    );
            }
        }
        (bw, bh)
    }

    /// Width of a laid-out line in physical px (for placing things after
    /// text without drawing it).
    fn measure(&mut self, text: &str, size: f64) -> f64 {
        self.layout_line(text, size).width() as f64
    }

    /// Block height of one line — parley's default line height ratio (~1.32)
    /// is close enough for plate geometry.
    fn line_height(&self, size: f64) -> f64 {
        size * 1.32 * self.scale
    }
}

// ---------------------------------------------------------------- 3D vectors

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).max(1e-9).sqrt();
    [v[0] / len, v[1] / len, v[2] / len]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// ------------------------------------------------------------------- layout

/// fibonacci sphere — even-ish distribution of `n` unit points.
fn fibonacci_sphere(n: usize) -> Vec<[f64; 3]> {
    let golden = std::f64::consts::PI * (3.0 - 5.0f64.sqrt());
    (0..n)
        .map(|i| {
            let y = 1.0 - 2.0 * (i as f64 + 0.5) / n as f64;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let th = golden * i as f64;
            [r * th.cos(), y, r * th.sin()]
        })
        .collect()
}

/// Flat-top hex cell in axial coordinates, hex circumradius `u`.
fn hex_to_pixel(q: f64, r: f64, u: f64) -> [f64; 2] {
    [u * 1.5 * q, u * 3f64.sqrt() * (r + q / 2.0)]
}

/// Every axial cell on hex ring `k` around the origin, walked side by side.
fn hex_ring(k: i32) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    let mut q = k;
    let mut r = -k;
    for (dq, dr) in [(1, 0), (0, -1), (-1, -1), (-1, 0), (-1, 1), (0, 1)] {
        for _ in 0..k {
            out.push((q, r));
            q += dq;
            r += dr;
        }
    }
    out
}

/// The honeycomb map: one ring-spiral patch per cluster, sitting on positions
/// around the map centre. `counts` holds each cluster's actual tile count, so
/// every tile lands in its own cluster's patch; the field radius derives from
/// the largest patch so neighbouring patches never touch.
fn layout_map(counts: &[usize]) -> Vec<[f64; 2]> {
    let clusters = counts.len();
    let u = 0.6; // cell circumradius inside a patch
    // a patch of m cells fills rings until 1 + 3k(k+1) >= m
    let max_cells = counts.iter().copied().max().unwrap_or(1);
    let mut ring = 0i32;
    while 1 + 3 * ring * (ring + 1) < max_cells as i32 {
        ring += 1;
    }
    // outer edge of the largest patch: furthest ring distance + one cell
    let patch_r = (3f64.sqrt() * ring as f64 + 1.0) * u;
    // eight patch centres on a circle: distance between neighbours is
    // 2·field_r·sin(π/8) ≈ 0.765·field_r — keep that above two patch radii
    let field_r = 2.8 * patch_r + 1.0;
    let mut out = Vec::with_capacity(counts.iter().sum());
    for (c, &per) in counts.iter().enumerate() {
        let ang = std::f64::consts::TAU * c as f64 / clusters as f64;
        let cx = field_r * ang.cos();
        let cy = field_r * ang.sin();
        let cells = std::iter::once((0, 0)).chain((1..).flat_map(hex_ring));
        for (q, r) in cells.take(per) {
            let [px, py] = hex_to_pixel(q as f64, r as f64, u);
            out.push([cx + px, cy + py]);
        }
    }
    out
}

/// One tile of the graph: a hexagon clad on the ball (unit normal `ball`),
/// its cell in the flat map, a search-name (its cluster) and a twinkle phase.
struct Node {
    cluster: usize,
    ball: [f64; 3],
    map: [f64; 2],
    phase: f64,
    /// deterministic per-tile brightness / size variation
    jitter: f64,
    /// 0 = brand-new file, 1 = old — drives how bright the tile burns
    age: f64,
    /// stand-in vault file name, shown when the tile gets focused
    file: &'static str,
}

impl Node {
    fn name(&self) -> &'static str {
        CLUSTER_NAMES[self.cluster]
    }
}

/// Eight cluster centres for the Voronoi-style region colouring.
fn cluster_centers() -> Vec<[f64; 3]> {
    fibonacci_sphere(8)
}

fn build_nodes() -> Vec<Node> {
    let n = TILE_COUNT;
    let centers = cluster_centers();
    let ball = fibonacci_sphere(n);
    let mut nodes: Vec<Node> = ball
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            // nearest centre wins → contiguous colour regions on the sphere
            let mut best = 0;
            let mut best_d = f64::MIN;
            for (c, ctr) in centers.iter().enumerate() {
                let d = dot(p, *ctr);
                if d > best_d {
                    best_d = d;
                    best = c;
                }
            }
            Node {
                cluster: best,
                ball: p,
                map: [0.0; 2], // filled in below after the cluster sort
                phase: (i as f64) * 0.618 * std::f64::consts::TAU,
                jitter: ((i * 2654435761) % 1000) as f64 / 1000.0,
                // golden-ratio sequence — well spread, decorrelated from jitter
                age: (i as f64 * 0.618_033_988_749_895).fract(),
                file: FILE_NAMES[best][i % FILE_NAMES[best].len()],
            }
        })
        .collect();
    // group by cluster so map patch `c` holds exactly cluster `c`'s tiles
    nodes.sort_by_key(|nd| nd.cluster);
    let mut counts = vec![0usize; 8];
    for nd in &nodes {
        counts[nd.cluster] += 1;
    }
    let map = layout_map(&counts);
    for (i, nd) in nodes.iter_mut().enumerate() {
        nd.map = map[i];
    }
    nodes
}

// ------------------------------------------------------------------- camera

/// Damped orbit camera. `t` blends towards the map: elevation sinks to 0,
/// distance grows — the fly-through through the ball into the honeycomb.
#[derive(Clone)]
struct Camera {
    azimuth: f64,
    elevation: f64,
    dist: f64,
    target: [f64; 3],
    map_zoom: f64,
    map_pan: [f64; 2],
}

impl Default for Camera {
    fn default() -> Self {
        Camera { azimuth: 0.6, elevation: 0.3, dist: 4.6, target: [0.0; 3], map_zoom: 18.0, map_pan: [0.0; 2] }
    }
}

/// Camera targets the camera *eases* towards (critically damped).
#[derive(Clone, Copy)]
struct CamTarget {
    azimuth: f64,
    elevation: f64,
    dist: f64,
    target: [f64; 3],
}

struct Sim {
    nodes: Vec<Node>,
    /// centre of each cluster's map patch (average of its cells' positions)
    cluster_centroids: Vec<[f64; 2]>,
    camera: Camera,
    cam_goal: CamTarget,
    /// free-orbit goal stashed away while the map is open, restored on exit
    orbit_goal: CamTarget,
    t: f64,
    t_goal: f64,
    spin: f64,
    spin_goal: f64,
    hover: Option<usize>,
    focus: Option<usize>,
    search: String,
    matches: Vec<usize>,
    time: f64,
}

impl Sim {
    fn new() -> Self {
        let nodes = build_nodes();
        let mut cluster_centroids = vec![[0.0f64; 2]; CLUSTER_NAMES.len()];
        let mut counts = vec![0usize; CLUSTER_NAMES.len()];
        for nd in &nodes {
            cluster_centroids[nd.cluster][0] += nd.map[0];
            cluster_centroids[nd.cluster][1] += nd.map[1];
            counts[nd.cluster] += 1;
        }
        for c in 0..cluster_centroids.len() {
            let n = counts[c].max(1) as f64;
            cluster_centroids[c][0] /= n;
            cluster_centroids[c][1] /= n;
        }
        Sim {
            nodes,
            cluster_centroids,
            camera: Camera::default(),
            cam_goal: CamTarget { azimuth: 0.6, elevation: 0.3, dist: 4.6, target: [0.0; 3] },
            orbit_goal: CamTarget { azimuth: 0.6, elevation: 0.3, dist: 4.6, target: [0.0; 3] },
            t: 0.0,
            t_goal: 0.0,
            spin: 0.0,
            // the current app yaws its ball at spin(0.02) × 3 = 0.06 rad/s
            spin_goal: 0.06,
            hover: None,
            focus: None,
            search: String::new(),
            matches: Vec::new(),
            time: 0.0,
        }
    }

    fn recompute_matches(&mut self) {
        self.matches = if self.search.is_empty() {
            Vec::new()
        } else {
            let q = self.search.to_lowercase();
            self.nodes
                .iter()
                .enumerate()
                .filter(|(_, nd)| nd.name().contains(&q))
                .map(|(i, _)| i)
                .collect()
        };
        if !self.search.is_empty() {
            eprintln!("search {:?} → {} tiles", self.search, self.matches.len());
        }
    }

    /// Fly to the search matches: aim at their centroid, close in on them.
    fn fly_to_matches(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        if self.t_goal > 0.5 {
            // in the map: pan onto the matches' patch — never touch the orbit
            // angles, they keep the map face-on
            let k = 1.0 / self.matches.len() as f64;
            let sum = self.matches.iter().fold([0.0; 2], |acc, &i| {
                let p = self.nodes[i].map;
                [acc[0] + p[0], acc[1] + p[1]]
            });
            self.cam_goal.target = [sum[0] * k, sum[1] * k, 0.0];
            self.focus = Some(self.matches[0]);
            return;
        }
        // node_pos, not the raw ball coords — the ball has already spun by the
        // time Enter is pressed
        let sum = self.matches.iter().fold([0.0; 3], |acc, &i| {
            let p = self.node_pos(i);
            [acc[0] + p[0], acc[1] + p[1], acc[2] + p[2]]
        });
        let k = 1.0 / self.matches.len() as f64;
        let c = [sum[0] * k, sum[1] * k, sum[2] * k];
        let r = (dot(c, c)).max(1e-6).sqrt();
        self.cam_goal.target = c;
        self.cam_goal.azimuth = c[2].atan2(c[0]);
        self.cam_goal.elevation = (c[1] / r).asin().clamp(-0.95, 0.95);
        self.cam_goal.dist = if self.matches.len() <= 60 { 3.4 } else { 4.2 };
        self.focus = Some(self.matches[0]);
    }

    fn clear_search(&mut self) {
        self.search.clear();
        self.matches.clear();
        self.focus = None;
        if self.t_goal > 0.5 {
            // in the map: recentre, but keep the face-on alignment
            self.cam_goal.target = [0.0; 3];
            self.cam_goal.azimuth = face_on_azimuth(self.cam_goal.azimuth);
            self.cam_goal.elevation = 0.0;
        } else {
            self.cam_goal = CamTarget {
                azimuth: self.cam_goal.azimuth,
                elevation: 0.3,
                dist: 4.8,
                target: [0.0; 3],
            };
        }
    }

    /// Enter the honeycomb map: stash the current orbit, then swing the camera
    /// to face the map dead-on (identity rotation — azimuth a multiple of 2π,
    /// elevation 0) so the flat cells are seen straight, never edge-on.
    fn enter_map(&mut self) {
        self.orbit_goal = self.cam_goal;
        self.cam_goal.azimuth = face_on_azimuth(self.cam_goal.azimuth);
        self.cam_goal.elevation = 0.0;
        self.cam_goal.target = [0.0; 3];
        self.t_goal = 1.0;
    }

    /// Leave the map: restore the orbit the ball view had before.
    fn leave_map(&mut self) {
        self.cam_goal = self.orbit_goal;
        self.t_goal = 0.0;
    }

    fn toggle_mode(&mut self) {
        // a view change ends the focus (and with it the spin pause)
        self.focus = None;
        if self.t_goal < 0.5 {
            self.enter_map();
        } else {
            self.leave_map();
        }
    }

    fn update(&mut self, dt: f64) {
        self.time += dt;
        // ambient rotation pauses while a tile is focused (search fly or
        // double-click) — same reason as the current app: the camera target is
        // computed once, and a ball rotating underneath it drifts away
        if self.focus.is_none() {
            self.spin += self.spin_goal * dt;
        }
        let damp = 1.0 - (-dt * 5.0).exp();
        let c = &mut self.camera;
        c.azimuth += (self.cam_goal.azimuth - c.azimuth) * damp;
        c.elevation += (self.cam_goal.elevation - c.elevation) * damp;
        c.dist += (self.cam_goal.dist - c.dist) * damp;
        for i in 0..3 {
            c.target[i] += (self.cam_goal.target[i] - c.target[i]) * damp;
        }
        self.t += (self.t_goal - self.t) * (1.0 - (-dt * 3.0).exp());
    }

    /// World centre of tile `i` for the current ball/map blend.
    fn node_pos(&self, i: usize) -> [f64; 3] {
        let nd = &self.nodes[i];
        let s = self.spin.sin();
        let c = self.spin.cos();
        let bx = nd.ball[0] * c + nd.ball[2] * s;
        let bz = -nd.ball[0] * s + nd.ball[2] * c;
        let e = ease_in_out(self.t);
        [
            bx * BALL_R * (1.0 - e) + (nd.map[0] + self.camera.map_pan[0]) * e,
            nd.ball[1] * BALL_R * (1.0 - e) + (nd.map[1] + self.camera.map_pan[1]) * e,
            bz * BALL_R * (1.0 - e),
        ]
    }

    /// Tile normal: the (spun) sphere normal blending to face +Z — the map
    /// lies in the XY plane, so its cells must stand *towards the camera*,
    /// not up — drives both the tangent frame and the headlight shading.
    fn node_normal(&self, i: usize) -> [f64; 3] {
        let nd = &self.nodes[i];
        let s = self.spin.sin();
        let c = self.spin.cos();
        let bx = nd.ball[0] * c + nd.ball[2] * s;
        let bz = -nd.ball[0] * s + nd.ball[2] * c;
        let e = ease_in_out(self.t);
        normalize([bx * (1.0 - e), nd.ball[1] * (1.0 - e), bz * (1.0 - e) + e])
    }

    fn matches_node(&self, i: usize) -> bool {
        self.search.is_empty() || self.matches.contains(&i)
    }
}

fn ease_in_out(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Nearest azimuth equivalent to "camera looks straight along +Z" — the view
/// rotation becomes the identity, which is what the flat map needs.
fn face_on_azimuth(azimuth: f64) -> f64 {
    (azimuth / std::f64::consts::TAU).round() * std::f64::consts::TAU
}

/// The camera as *viewed*: the eased orbit plus the ball's own slow nod — the
/// current app tilts with `ax = 0.35 + sin(t·0.15)·0.08`. The sway fades out
/// in the map, whose view must stay dead-on.
fn view_camera(sim: &Sim) -> Camera {
    let mut cam = sim.camera.clone();
    let sway = 0.08 * (sim.time * 0.15).sin() * (1.0 - ease_in_out(sim.t));
    cam.elevation += sway;
    cam
}

/// Pixels per graph unit at the current view blend. Ball: fixed px-per-unit
/// so the sphere fills the window; map: the ortho zoom. Combined with the
/// perspective distance in `project`, this yields the usual focal length.
fn px_per_unit(cam: &Camera, t: f64) -> f64 {
    // With BALL_R = 1.55 and dist 4.6 the silhouette lands at ~355px radius
    // in the default window — the ball fills the view like the original
    // dashboard, with 384 tiles at ~40px on it
    let ball = 215.0;
    let map = cam.map_zoom;
    ball * (1.0 - t) + map * t
}

/// Rotate a world position into view space (azimuth around Y, then elevation
/// around X), relative to the camera target.
fn to_view(cam: &Camera, p: [f64; 3]) -> [f64; 3] {
    let x = p[0] - cam.target[0];
    let y = p[1] - cam.target[1];
    let z = p[2] - cam.target[2];
    let (sa, ca) = cam.azimuth.sin_cos();
    let x1 = x * ca - z * sa;
    let z1 = x * sa + z * ca;
    let (se, ce) = cam.elevation.sin_cos();
    [x1, y * ce - z1 * se, y * se + z1 * ce]
}

/// Project a world-space position to screen px; returns (x, y, k) with
/// k = pixels per world unit at that depth. `t` is the raw ball↔map blend.
fn project(cam: &Camera, t: f64, size: (f64, f64), p: [f64; 3]) -> (f64, f64, f64) {
    let (w, h) = size;
    let v = to_view(cam, p);
    let t = ease_in_out(t);
    let dist = cam.dist * (1.0 + 2.4 * t);
    let denom = (dist - v[2]).max(0.2);
    let focal = px_per_unit(cam, t) * dist;
    let k = focal / denom;
    (w / 2.0 + v[0] * k, h / 2.0 + v[1] * k, k)
}

/// Draw one full frame: background, tiles back-to-front, HUD strip.
///
/// A free function taking `&mut Scene` and `&Sim` separately so the borrows
/// stay disjoint (a method on `&mut State` would fight over the whole self);
/// `text` is disjoint from both by construction.
fn draw_into(scene: &mut Scene, sim: &Sim, size: (f64, f64), text: &mut TextKit) {
    let (w, h) = size;
    let t = ease_in_out(sim.t);
    // eased orbit + the ball's slow nod, faded out as the map takes over
    let cam = view_camera(sim);
    let hovered = sim.hover;
    let focus = sim.focus;
    let searching = !sim.search.is_empty();

    // Painter's order: far tiles first (k doubles as depth key: nearer = larger).
    let mut order: Vec<(usize, f64, f64, f64, f64)> = Vec::with_capacity(sim.nodes.len());
    for i in 0..sim.nodes.len() {
        let (x, y, k) = project(&cam, sim.t, size, sim.node_pos(i));
        order.push((i, x, y, k, k));
    }
    order.sort_by(|a, b| a.4.partial_cmp(&b.4).unwrap());

    scene.reset();
    scene.fill(Fill::NonZero, Affine::IDENTITY, BG, None, &Rect::new(0.0, 0.0, w, h));

    // tile world radius: ~0.13 of the ball like the original (a grooved
    // mosaic with dark crevices), cell-filling on the map (cells are
    // circumradius 0.6 in `layout_map`)
    let radius_world = 0.20 * (1.0 - t) + 0.63 * t;

    for (i, cxp, cyp, ck, _) in order {
        let nd = &sim.nodes[i];
        let centre = sim.node_pos(i);
        let normal = sim.node_normal(i);
        let twinkle = 0.94 + 0.06 * (sim.time * 0.9 + nd.phase).sin();
        let matched = !searching || sim.matches_node(i);
        let mut alpha = twinkle * if matched { 1.0 } else { 0.14 };
        let hot = hovered == Some(i) || focus == Some(i);
        if hot {
            alpha = 1.0;
        }

        // headlight: front-facing tiles bright, rim dark; flat in the map
        let nv = to_view(&cam, normal);
        let lit = (0.30 + 0.70 * nv[2].clamp(0.0, 1.0)) * (1.0 - t) + 1.0 * t;
        // Recency drives brightness: new files burn bright (and glow), old
        // ones sink into the dome — the cluster colour stays the hue identity,
        // unlike the original, where age and colour tangle
        let recency = 1.0 - nd.age;
        let lit_up = hot || (searching && matched);
        let on = if lit_up { 1.0 } else { (0.32 + 0.68 * recency.powf(1.4)) * (1.0 - t) + t };
        // Farbwechsel: a small hue drift tied to the tile's view angle plus a
        // slow per-tile cycle — colours wander as the ball turns
        let phi = nv[1].atan2(nv[0]);
        let hue = 0.14 * phi + 0.10 * (sim.time * 0.2 + nd.phase).sin();
        let raw = if searching && matched {
            [242, 200, 90, 255]
        } else {
            CLUSTER_COLORS[nd.cluster]
        };
        let base = hue_rotate(mix(raw, [26, 27, 32, 255], 0.72 * (1.0 - on)), hue);
        let brightness =
            lit * (0.92 + 0.16 * nd.jitter) * if hot { 1.25 } else { 1.0 } * (0.55 + 0.45 * on);
        // metallic sheen stops: white-tinted light side → flat → deep dark
        // side; the contrast collapses on dormant tiles and in the flat map
        let sheen = (1.0 - t) * on;
        let lit_col = shade(base, brightness, alpha);
        let dark_col = shade(base, brightness * (1.0 - 0.45 * sheen), alpha);
        let light_col = shade(mix_white(base, 0.5 * sheen), brightness * 1.15, alpha);

        // hexagon in the tangent plane of `normal`
        let up = if normal[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let u = normalize(cross(up, normal));
        let v = cross(normal, u);
        let radius = radius_world * if hot { 1.08 } else { 1.0 + 0.08 * nd.jitter };
        // every tile turns slowly on its own axis — most visibly the front-
        // facing ones in the middle; unwinds as the flat map forms
        let rot = (sim.time * (0.22 + 0.10 * nd.jitter) + nd.phase) * (1.0 - t);
        let mut pts = Vec::with_capacity(6);
        // Drape strength: on the ball the tangent hexagon is pulled back onto
        // the sphere — flat corners poke past the silhouette and show black
        // cracks once zoomed in. In the map the cell stays flat.
        let drape = 1.0 - t;
        let centre_len = dot(centre, centre).sqrt();
        for ci in 0..6 {
            let a = std::f64::consts::TAU * (ci as f64) / 6.0 + rot;
            let (sa, ca) = a.sin_cos();
            let d = [u[0] * ca + v[0] * sa, u[1] * ca + v[1] * sa, u[2] * ca + v[2] * sa];
            let raw =
                [centre[0] + d[0] * radius, centre[1] + d[1] * radius, centre[2] + d[2] * radius];
            // radial scale blending 1 → |centre|/|raw|: corners land exactly
            // on the sphere of the tile's centre radius
            let rl = dot(raw, raw).sqrt().max(1e-9);
            let f = 1.0 + drape * (centre_len / rl - 1.0);
            let world = [raw[0] * f, raw[1] * f, raw[2] * f];
            let (px, py, _) = project(&cam, sim.t, size, world);
            pts.push((px, py));
        }
        let mut path = BezPath::new();
        for (pi, (px, py)) in pts.iter().enumerate() {
            if pi == 0 {
                path.move_to((*px, *py));
            } else {
                path.line_to((*px, *py));
            }
        }
        path.close_path();
        // glow: a soft halo behind the brightest tiles — the newest files
        // bloom hardest, kept subtle (ball only; the flat map stays clean)
        if on > 0.68 && t < 0.5 {
            let strength = ((on - 0.68) / 0.32).min(1.0);
            let halo_r = (ck * radius_world * (1.7 + 0.6 * strength)).max(8.0);
            let halo = Gradient::new_radial((cxp, cyp), halo_r as f32).with_stops([
                (0.0f32, Color::from_rgba8(raw[0], raw[1], raw[2], (65.0 * strength * alpha) as u8)),
                (1.0, Color::from_rgba8(raw[0], raw[1], raw[2], 0)),
            ]);
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &Brush::Gradient(halo),
                None,
                &Circle::new((cxp, cyp), halo_r),
            );
        }
        // metallic sheen: the gradient's axis follows the tile's own rotation,
        // so the highlight sweeps across while the tile spins — brushed metal
        // under a fixed light
        let ga = rot - 2.156; // sheen from the upper-left at rest
        let (sdx, sdy) = (ga.cos(), ga.sin());
        let r_px = (ck * radius_world).max(2.0);
        let grad = Gradient::new_linear(
            Point::new(cxp + sdx * r_px, cyp + sdy * r_px),
            Point::new(cxp - sdx * r_px, cyp - sdy * r_px),
        )
        .with_stops([(0.0f32, light_col), (0.45, lit_col), (1.0, dark_col)]);
        scene.fill(Fill::NonZero, Affine::IDENTITY, &Brush::Gradient(grad), None, &path);
        // dark grout between tiles, like the current rendering
        scene.stroke(
            &Stroke::new(1.4),
            Affine::IDENTITY,
            &Brush::Solid(Color::from_rgba8(4, 5, 6, ((120.0 * alpha).min(255.0)) as u8)),
            None,
            &path,
        );
    }

    // ------------------------------------------------------------------ HUD
    // All text goes through `TextKit` (parley layout → vello glyph runs).
    let bone = [206u8, 200, 194, 235];
    let muted = [148u8, 142, 136, 165];
    let faint = [148u8, 142, 136, 95];
    let amber = [242u8, 200, 90, 240];
    let s = text.scale;

    // title, top-left — the current dashboard's header, condensed
    text.draw(scene, "Second Brain", 15.0, (18.0 * s, 14.0 * s), bone, Align::Left);
    text.draw(
        scene,
        &format!("{} Dateien · {} Bereiche", sim.nodes.len(), CLUSTER_NAMES.len()),
        10.0,
        (19.0 * s, 36.0 * s),
        faint,
        Align::Left,
    );

    // view word, top-right
    let mode = if t > 0.5 { "Karte" } else { "Kugel" };
    text.draw(scene, mode, 10.5, (w - 18.0 * s, 16.0 * s), faint, Align::Right);

    // cluster labels on the flat map — fading in with the map itself
    if t > 0.35 {
        let la = ((t - 0.35) / 0.65).clamp(0.0, 1.0);
        for (c, ctr) in sim.cluster_centroids.iter().enumerate() {
            let (x, y, _) = project(
                &cam,
                sim.t,
                size,
                [ctr[0] + sim.camera.map_pan[0], ctr[1] + sim.camera.map_pan[1], 0.0],
            );
            let col = mix_white(CLUSTER_COLORS[c], 0.4);
            text.draw(
                scene,
                &CLUSTER_NAMES[c].to_uppercase(),
                11.0,
                (x, y - 6.0 * s),
                [col[0], col[1], col[2], (200.0 * la) as u8],
                Align::Center,
            );
        }
    }

    // focus chip, top-centre — the file a double-clicked (or flown-to) tile
    // stands for, with its cluster and an age band (ball view only)
    if let Some(i) = focus {
        if sim.t < 0.5 {
            let nd = &sim.nodes[i];
            let age_word = match nd.age {
                a if a < 0.12 => "frisch",
                a if a < 0.3 => "diese Woche",
                a if a < 0.5 => "diesen Monat",
                a if a < 0.75 => "älter",
                _ => "archiviert",
            };
            let tail = format!(" · {} · {}", nd.name(), age_word);
            let nw = text.measure(nd.file, 12.0);
            let tw = text.measure(&tail, 10.0);
            let total = nw + tw + 8.0 * s;
            let cx = w / 2.0;
            let bh = text.line_height(12.0);
            let top = 12.0 * s;
            let plate = RoundedRect::from_rect(
                Rect::new(cx - total / 2.0 - 12.0 * s, top, cx + total / 2.0 + 12.0 * s, top + bh + 11.0 * s),
                9.0 * s,
            );
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &Brush::Solid(Color::from_rgba8(16, 17, 21, 215)),
                None,
                &plate,
            );
            scene.stroke(
                &Stroke::new(1.0),
                Affine::IDENTITY,
                &Brush::Solid(Color::from_rgba8(90, 88, 84, 110)),
                None,
                &plate,
            );
            let col = mix_white(CLUSTER_COLORS[nd.cluster], 0.25);
            text.draw(scene, nd.file, 12.0, (cx - total / 2.0, top + 5.0 * s), [col[0], col[1], col[2], 245], Align::Left);
            text.draw(scene, &tail, 10.0, (cx - total / 2.0 + nw + 8.0 * s, top + 7.2 * s), muted, Align::Left);
        }
    }

    // search HUD, bottom-centre: a real field while a query is live, a quiet
    // hint line otherwise
    let field_w = 520.0 * s;
    let field_h = text.line_height(13.0) + 12.0 * s;
    let fy = h - 30.0 * s - field_h;
    if searching {
        let plate = RoundedRect::from_rect(
            Rect::new(w / 2.0 - field_w / 2.0, fy, w / 2.0 + field_w / 2.0, fy + field_h),
            10.0 * s,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(Color::from_rgba8(14, 15, 18, 235)),
            None,
            &plate,
        );
        // amber frame while a query is live
        scene.stroke(
            &Stroke::new(1.0 * s),
            Affine::IDENTITY,
            &Brush::Solid(Color::from_rgba8(242, 200, 90, 140)),
            None,
            &plate,
        );
        let ix = w / 2.0 - field_w / 2.0 + 14.0 * s;
        let iy = fy + 6.0 * s;
        let qw = text.draw(scene, &sim.search, 13.0, (ix, iy), if sim.matches.is_empty() { amber } else { bone }, Align::Left).0;
        // blinking caret after the query
        if (sim.time * 2.4).fract() < 0.6 {
            let caret = Rect::new(ix + qw + 2.0 * s, iy + 1.0 * s, ix + qw + 4.0 * s, iy + text.line_height(13.0) - 2.0 * s);
            scene.fill(Fill::NonZero, Affine::IDENTITY, &Brush::Solid(Color::from_rgba8(242, 200, 90, 200)), None, &caret);
        }
        // match count, right side of the field
        text.draw(
            scene,
            &format!("{}/{} Zellen", sim.matches.len(), sim.nodes.len()),
            10.0,
            (w / 2.0 + field_w / 2.0 - 14.0 * s, fy + 8.5 * s),
            muted,
            Align::Right,
        );
        text.draw(
            scene,
            "↵ fliegt zu den Treffern · ESC zurück",
            9.0,
            (w / 2.0, fy + field_h + 5.0 * s),
            faint,
            Align::Center,
        );
    } else {
        text.draw(
            scene,
            "Klick: Kugel ↔ Karte · Doppelklick: Zelle öffnen · Tippen: suchen",
            9.5,
            (w / 2.0, h - 20.0 * s),
            faint,
            Align::Center,
        );
    }
}

// --------------------------------------------------------------------- state

struct State {
    window: Arc<Window>,
    context: RenderContext,
    surface: RenderSurface<'static>,
    renderer: Renderer,
    scene: Scene,
    text: TextKit,
    sim: Sim,
    size: (f64, f64),
    cursor: (f64, f64),
    dragging: Option<(MouseButton, (f64, f64))>,
    /// set while a drag exceeded the movement threshold — suppresses the
    /// click toggle on release
    drag_moved: bool,
    last_click: Option<Instant>,
    last_frame: Option<Instant>,
}

impl State {
    async fn new(window: Arc<Window>) -> Result<Self> {
        let mut context = RenderContext::new();
        let size = window.inner_size();
        let surface = context
            .create_surface(window.clone(), size.width, size.height, vello::wgpu::PresentMode::AutoVsync)
            .await
            .context("create_surface")?;
        let renderer = Renderer::new(
            &context.devices[surface.dev_id].device,
            RendererOptions {
                use_cpu: false,
                num_init_threads: std::num::NonZeroUsize::new(1),
                ..Default::default()
            },
        )
        .context("vello Renderer")?;
        let scale = window.scale_factor();
        Ok(State {
            window,
            context,
            surface,
            renderer,
            scene: Scene::new(),
            text: TextKit::new(scale),
            sim: Sim::new(),
            size: (size.width as f64, size.height as f64),
            cursor: (0.0, 0.0),
            dragging: None,
            drag_moved: false,
            last_click: None,
            last_frame: None,
        })
    }

    fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.size = (w as f64, h as f64);
        // vello recreates the offscreen target (with its own format + usage
        // flags) and reconfigures the surface in one go
        self.context.resize_surface(&mut self.surface, w, h);
    }

    fn draw(&mut self) {
        let Self { scene, sim, size, text, .. } = self;
        draw_into(scene, sim, *size, text);
    }

    fn render(&mut self) -> Result<()> {
        self.draw();
        let dev_id = self.surface.dev_id;
        let device = &self.context.devices[dev_id].device;
        let queue = &self.context.devices[dev_id].queue;
        let surface_texture = match self.surface.surface.get_current_texture() {
            vello::wgpu::CurrentSurfaceTexture::Success(st) | vello::wgpu::CurrentSurfaceTexture::Suboptimal(st) => st,
            // timeout / occluded / lost: skip this frame, the redraw loop retries
            _ => return Ok(()),
        };
        self.renderer.render_to_texture(
            device,
            queue,
            &self.scene,
            &self.surface.target_view,
            &vello::RenderParams {
                base_color: BG,
                width: self.size.0 as u32,
                height: self.size.1 as u32,
                antialiasing_method: AaConfig::Msaa8,
            },
        )?;
        let surface_view = surface_texture.texture.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&vello::wgpu::CommandEncoderDescriptor {
            label: Some("hexball blit"),
        });
        self.surface
            .blitter
            .copy(device, &mut encoder, &self.surface.target_view, &surface_view);
        queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        queue.present(surface_texture);
        Ok(())
    }

    /// Screen-space hit test: nearest tile centre within a generous radius.
    fn hit(&self) -> Option<usize> {
        let (mx, my) = self.cursor;
        let cam = view_camera(&self.sim);
        let mut best: Option<(usize, f64)> = None;
        for i in 0..self.sim.nodes.len() {
            let (x, y, k) = project(&cam, self.sim.t, self.size, self.sim.node_pos(i));
            let d = ((x - mx).powi(2) + (y - my).powi(2)).sqrt();
            let r = 22.0 * k;
            if d < r && best.map_or(true, |(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        best.map(|(i, _)| i)
    }
}

// -------------------------------------------------------------------- app

struct App {
    state: Option<State>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, elw: &ActiveEventLoop) {
        if self.state.is_none() {
            let window = Arc::new(
                elw.create_window(
                    Window::default_attributes()
                        .with_title("Axiomata · Hexball (vello prototype)")
                        .with_inner_size(LogicalSize::new(1280.0, 840.0))
                        // Explicit position: macOS may open default-positioned
                        // windows on a space or display the user isn't looking at.
                        .with_position(winit::dpi::LogicalPosition::new(160.0, 120.0))
                        .with_visible(true),
                )
                .expect("create window"),
            );
            let state = pollster::block_on(State::new(window)).expect("init vello");
            self.state = Some(state);
        }
    }

    fn window_event(&mut self, elw: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(st) = self.state.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => elw.exit(),
            WindowEvent::Resized(size) => st.resize(size.width, size.height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // text sizes are logical px — follow the display density
                st.text.scale = scale_factor;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (px, py) = (position.x, position.y);
                if let Some((btn, (lx, ly))) = st.dragging {
                    let (dx, dy) = (px - lx, py - ly);
                    if dx.abs() + dy.abs() > 0.0 {
                        // accumulate from the drag origin: st.dragging.1 is the
                        // last position, so track total movement separately
                        st.drag_moved = st.drag_moved || (dx * dx + dy * dy).sqrt() > 3.0;
                    }
                    if btn == MouseButton::Left {
                        if st.sim.t_goal < 0.5 {
                            st.sim.cam_goal.azimuth += dx * 0.008;
                            // ±60°: past that, horizontal dragging degenerates
                            // into a screen-plane spin — the ball turns like a
                            // clock hand instead of yawing
                            st.sim.cam_goal.elevation =
                                (st.sim.cam_goal.elevation + dy * 0.006).clamp(-1.05, 1.05);
                            // dragging steers the camera directly
                            st.sim.camera.azimuth = st.sim.cam_goal.azimuth;
                            st.sim.camera.elevation = st.sim.cam_goal.elevation;
                        } else {
                            st.sim.camera.map_pan[0] -= dx / st.sim.camera.map_zoom;
                            st.sim.camera.map_pan[1] -= dy / st.sim.camera.map_zoom;
                        }
                    }
                    st.dragging = Some((btn, (px, py)));
                }
                st.cursor = (px, py);
                st.sim.hover = st.hit();
                st.window.request_redraw();
            }
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                match btn_state {
                    ElementState::Pressed => {
                        if button == MouseButton::Left {
                            let dbl = st.last_click.map(|t| t.elapsed().as_millis() < 280).unwrap_or(false);
                            st.last_click = Some(Instant::now());
                            if dbl {
                                // double click: focus the tile under the cursor
                                if let Some(i) = st.hit() {
                                    st.sim.focus = Some(i);
                                    let p = st.sim.node_pos(i);
                                    st.sim.cam_goal.target = p;
                                    st.sim.cam_goal.dist = 3.2;
                                }
                            } else {
                                st.dragging = Some((button, st.cursor));
                                st.drag_moved = false;
                            }
                        }
                    }
                    ElementState::Released => {
                        if button == MouseButton::Left {
                            let toggle = !st.drag_moved;
                            st.dragging = None;
                            st.drag_moved = false;
                            // simple click (no drag): toggle ball <-> map
                            if toggle {
                                st.sim.toggle_mode();
                            }
                        }
                    }
                }
                st.window.request_redraw();
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(p) => p.y / 40.0,
                };
                if st.sim.t_goal < 0.5 {
                    // 3.0 keeps the camera outside the tile shell of the 1.55-
                    // radius ball — closer and you fall between the hexagons
                    st.sim.cam_goal.dist = (st.sim.cam_goal.dist * (1.0 - d * 0.12)).clamp(3.0, 12.0);
                } else {
                    st.sim.camera.map_zoom = (st.sim.camera.map_zoom * (1.0 + d * 0.15)).clamp(6.0, 90.0);
                }
                st.window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        if !st.sim.search.is_empty() {
                            st.sim.clear_search();
                        } else if st.sim.t_goal > 0.5 {
                            st.sim.leave_map();
                        }
                    }
                    Key::Named(NamedKey::Enter) => st.sim.fly_to_matches(),
                    Key::Named(NamedKey::Backspace) => {
                        st.sim.search.pop();
                        st.sim.recompute_matches();
                    }
                    Key::Named(NamedKey::Space) => {
                        // space is a common word separator — keep it in queries
                        st.sim.search.push(' ');
                        st.sim.recompute_matches();
                    }
                    Key::Character(c) => {
                        st.sim.search.push_str(c.as_str());
                        st.sim.recompute_matches();
                    }
                    _ => {}
                }
                st.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                // dt from the previous frame; first frame ~16ms
                let dt = st
                    .last_frame
                    .replace(now)
                    .map(|last| now.duration_since(last).as_secs_f64().min(0.1))
                    .unwrap_or(1.0 / 60.0);
                st.sim.update(dt);
                if let Err(e) = st.render() {
                    eprintln!("hexball: render error: {e:#}");
                }
                st.window.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _elw: &ActiveEventLoop) {
        if let Some(st) = &self.state {
            st.window.request_redraw();
        }
    }
}

fn main() -> Result<()> {
    let event_loop = EventLoop::new().context("event loop")?;
    let mut app = App { state: None };
    event_loop.run_app(&mut app).context("run")?;
    Ok(())
}

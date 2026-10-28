# Glas — Axiomata stellt auf eine Glas-Oberfläche um

Stand: 2026-10-28, **gegrillt** (G1–G14, Owner); GL0 vom Owner abgenommen, Bau ab GL1. Angeregt durch PandaCode; Mockups in
`docs/plans/glass/` (`glass-mockup.html` mit den Schichten `glass.css` = Glas und `glass2.css` = Liquid). Baut auf
„Glut + Hex“ auf (`docs/plans/ui-ux-round.md`: Akzentregel, `ui/`-Familie, `core/motion.ts`). Dieser Plan ist
zugleich die **Vorgabe für N3** (`docs/plans/axiomata-next.md`: Terminal, Kanban und Studio in reinem Rust) — die
Werte und Regeln in §3 gelten für den Neubau unverändert.

## 1. Grundsatz

Nur Flächen, Licht und Tiefe ändern sich, **nie die Anordnung**. Panes und Panels werden halbtransparentes Glas mit
einer Lichtkante statt eines Rahmens; dahinter liegt ein Grund, den das Glas bricht (Waben über Glutlicht). Es gibt
**keinen Schalter** (G4): ganz Axiomata stellt um, in allen Themes — kein zweiter Weg neben dem Glas.

## 2. Entscheidungen (Owner, 2026-10-28)

- **G1 Glas + zwei Teile von Liquid.** Grundlage ist die Glas-Schicht; aus Liquid kommen nur die **lebenden Ringe**
  (G11) und das **weiche Harness-Licht** oben in den Agent-Kacheln (statt des harten 3-px-Balkens). Keine Körnung,
  kein Glanz (teuer zu zeichnen, unruhig hinter langem Text).
- **G2 Jetzt in Svelte, zugleich Vorgabe für N3.** Ein Token-Satz `--ax-glass-*`, sauber in die Komponenten
  eingebaut — keine `!important`-Schicht wie im Mockup.
- **G3 Geltungsbereich:** Studio, Kanban, Datei/Notes und **alle** schwebenden Panels (`shell/Window.svelte`,
  Staging-Panels, Spotlight, Einstellungen) — eine Plattensprache. Die Orbit-Kacheln behalten ihr eigenes Glas
  (`--ax-tile-glass-*`).
- **G4 Kein Schalter.** Die App stellt um, jedes der 14 Themes bekommt Glaswerte.
- **G5 Deckkraft je Fläche + ein Regler.** Panes/Panels ≈ 46 % (dunkel), Leseflächen (Editor, Diff, Notes) als
  dichte, ruhige Platte darüber. Ein Regler „Glass“ unter Appearance verschiebt beide gemeinsam (wie die
  Kachel-Deckkraft); ganz rechts = dicht (= G12).
- **G6 Helle Themes: Milchglas** — weiße Fläche mit höherer Grunddeckkraft als im Dunkeln, dunklere Haarlinie,
  weiße Lichtkante oben, weichere Schatten. Vor dem Bau ein Mockup Paper + Latte (GL0).
- **G7 Der Grund.** Ein Theme kann `--ax-glass-backdrop` setzen (Forest grünlich, Ocean bläulich, Steampunk Messing
  …); sonst gilt das Glutlicht in `--ax-accent`: fest unten links, schwach oben rechts, unter dem Wabenmuster. Es
  wandert nicht — wo etwas arbeitet, sagen die Ringe.
- **G8 Schwebende Platten.** Studio-Panes mit 3 px Luft und 14 px Radius; erst die Luft zeigt den Grund.
- **G9 Titel in normaler Schreibung, app-weit** — keine gesperrten Versalien mehr in Bereichsköpfen.
- **G10 Orbit:** dasselbe Glutlicht liegt unter seinen Waben; die Tools bleiben unverändert.
- **G11 Der Ring ist die neue Gestalt von `use:glow`** (`ui/glow.ts`) ab Kartengröße: arbeitet → langsam
  umlaufender Glutring, wartet auf den Owner → ruhiger Bernsteinring. Kleine Elemente (Status-Punkte, Tabs, Chips)
  behalten das heutige Glimmen. Calm/Off (`core/motion.ts`) halten den Ring still.
- **G12 macOS „Transparenz reduzieren“** macht das Glas dicht (ohne Unschärfe, volle Deckkraft), automatisch wie
  „Bewegung reduzieren“ → Motion Off; kein eigener Schalter.
- **G13 Terminal blickdicht:** die Zeichenfläche des Terminals (Agent-Panes, Run, Master, Glut-TUI) in der
  Hintergrundfarbe des Themes; die Pane-Platte drumherum bleibt Glas. Keine Unschärfe hinter schnell zeichnender
  Ausgabe, keine Farbabweichung zur TUI.
- **G14 Zwei Funde aus dem Mockup als Regel:** Menüs und Popover (`role=menu|listbox`, Projektwahl) bekommen dichtes
  Glas (88 %), sonst mischt sich ihr Text mit dem darunter. Versteckte Ansichten sind `visibility: hidden`, nicht nur
  verdeckt — sonst scheint z. B. ein offener Diff durch die Flow-Panes.

## 3. Spec (auch für N3)

Werte für dunkle Themes (aus `glass.css`); die hellen (Paper, Latte, GitHub Light) aus `glass-light.css`, abgenommen
in GL0: Pane = `--ax-surface-2` zu 56 %, Panel zu 68 %, Kante = `--ax-text` zu 10 %, Lichtkante `rgba(255,255,255,.9)`,
Platte `rgba(255,255,255,.55)`, Lesefläche weißes Blatt `.55`, Menü `--ax-surface-2` zu 92 %, Schatten warm und flach
(10–16 %), Glutlicht `--ax-accent` zu 48 % / 22 % mit kühlem Gegenlicht `rgba(110,125,175,.16)`:

| Token | Wert (dunkel) | Wofür |
|---|---|---|
| `--ax-glass-pane` | `rgba(22,22,28,.46)` | Panes, Seitenleisten |
| `--ax-glass-panel` | `rgba(20,20,26,.50)` | schwebende Panels |
| `--ax-glass-read` | `rgba(0,0,0,.20)` | Leseflächen über dem Glas (Editor, Diff, Notes) |
| `--ax-glass-menu` | `rgba(24,24,30,.88)` | Menüs, Popover |
| `--ax-glass-blur` | `blur(24px) saturate(160%)` (Panels 28px/170%) | Unschärfe |
| `--ax-glass-edge` | `rgba(255,255,255,.075)` | 1-px-Haarlinie statt Rahmen |
| `--ax-glass-light` | `inset 0 1px 0 rgba(255,255,255,.06)` | Lichtkante oben |
| `--ax-glass-plate` / `-hover` | `rgba(255,255,255,.04)` / `.065` | Karten, Hinweise, Plan-Einträge (Radius 12 px) |
| `--ax-glass-backdrop` | Glutlicht in `--ax-accent` + Waben | der Grund |
| `--ax-glass-gap` / `-radius` | 3 px / 14 px | schwebende Platten |

Der Regler (G5) skaliert die Deckkraft von pane/panel/read gemeinsam; „Transparenz reduzieren“ (G12) setzt sie auf
1 und die Unschärfe auf `none`. Kanban-Spalten haben keinen Boden, nur die Karten sind Platten.

## 4. Checkpoints

Je Checkpoint ein Commit mit dem kombinierten Review davor; Bau in einem eigenen Worktree (laufende Agenten des
Owners). Halt nur bei GL0 und nach GL2.

- **GL0 — Mockup heller Themes:** Paper + Latte mit Milchglas im Browser-Mock (`glass/glass-light-mockup.html`) —
  **abgenommen** 2026-10-28.
- **GL1 — Fundament:** `--ax-glass-*` in `themes/tokens.css` und allen 14 Themes, `--ax-glass-backdrop` je Theme,
  der Regler unter Appearance, „Transparenz reduzieren“ (`axiomata-macos`), versteckte Ansichten `visibility: hidden`.
- **GL2 — Studio:** schwebende Platten, Leseflächen, Terminal blickdicht (G13), Menüs dicht (G14). Danach
  **Leistungstest** in der installierten App (Studio mit laufenden Agent-Panes, Scrollen in Editor und Kanban,
  Zeitmessung im Safari-Web-Inspector, Owner als „Hände“); ruckelt es, wird zuerst die Unschärfe angepasst.
- **GL3 — Panels:** `Window.svelte`, Staging-Panels, Kanban (Spalten ohne Boden, Kartenplatten), Datei/Notes,
  Spotlight, Einstellungen; der Orbit-Grund (G10).
- **GL4 — Lebende Ringe:** `use:glow` wird ab Kartengröße zum Ring (G11), Harness-Licht oben in den Agent-Kacheln.
- **GL5 — Titel + Doku:** normale Schreibung app-weit (G9); `docs/architecture.md` §7, AGENTS.md (Glas-Regel neben
  der Akzentregel), Querverweis in `ui-ux-round.md`.

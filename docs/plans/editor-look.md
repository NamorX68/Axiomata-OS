# Editor: drei Erweiterungen und eine moderne Optik — der künftige Standard für Axiomata

Stand: 2026-09-28, gegrillt (Runden 1–2) und vom Owner bestätigt; Umsetzung beginnt mit LK0. Anlass (Owner): „Zur Zeit sieht das alles nach einer
alten Linux-X-Anwendung aus und nicht wie ein moderner Editor.“ Vorbild ist vor allem Zed, daneben VS Code und
JetBrains. Was hier für die Datei-App und die IDE entsteht, wird später Modul für Modul zum Standard der ganzen App.
Vorgänger: `docs/plans/editor.md` (ED0–ED6).

## 1. Ziele

- **Drei Erweiterungen**: eine Tastenkürzel-Anzeige, eine immer sichtbare Minimap, mehrere Reiter nebeneinander.
- **Optik**: Reiter mit Rundungen, Icons im Dateibaum (vier Stile), Icon-Knöpfe statt Textzeichen, eine stärkere
  Cursor-Animation.
- **Lesbarkeit**: Die UI-Schrift passt sich der Bildschirmauflösung an. Befund des Owners: Auf dem 5K-Monitor, der
  fast nativ skaliert ist, passen Überschriften und die erste Ebene, alles darunter ist zu klein — app-weit
  (Kanban-Unterebenen, der Knopf „groß öffnen“ in der Kanban-Kachel, die gerenderte Markdown-Ansicht im FilePeek, in
  der Mail-Zusammenfassung und im Orbit). Die Schriftart und -größe des Editors bleiben unberührt.

## 2. Entscheidungen

- **K1 — Reihenfolge** (R1-Q1): Ein gemeinsamer Plan. Zuerst ein schmales Fundament (LK0), dann die drei
  Erweiterungen, gleich im neuen Stil, dann der Rest der Datei-App und der IDE. Die Optik für den Rest der App wird
  später je Modul eigens geplant.
- **K2 — Inspector** (R1-Q2): Die rechte Spalte der Datei-App wird ein „Inspector“ mit den Reitern **Settings |
  Shortcuts**, geöffnet über zwei Icon-Knöpfe in der Titelleiste. Die Tastenkürzel-Liste wird aus den echten
  Belegungen erzeugt (`editor/keymap.ts`, Vi, Datei-App, LSP-Tasten), nie von Hand gepflegt. Sie ist nach Bereichen
  gruppiert und durchsuchbar; den Vi-Teil zeigt sie nur, wenn Vi an ist.
- **K3 — Minimap** (R1-Q3): Sie bleibt am Text. Der Inspector steht rechts *neben* ihr, der Text wird schmaler.
- **K4 — Split** (R1-Q4): Die Datei-App übernimmt das Dock-Modell der IDE (`src/ide/layout.ts`, M7.1): beliebig
  teilen, nebeneinander und untereinander, ⌘\ teilt, ein Reiter lässt sich an einen Rand ziehen. Dieselbe Datei in
  zwei Gruppen teilt sich ein Dokument.
- **K5 — Datei-Icons** (R1-Q5, R2-Q5): Frei lizenzierte SVG-Sätze werden als Teilmenge eingecheckt, mit
  Lizenzhinweis (wie Schriften und Grammatiken): **Catppuccin** (MIT), **JetBrains Expui** (Apache 2.0), **Git** =
  GitHubs **Octicons** (MIT; beim Bau gegen Zeds nachladbare Icon-Themes abgleichen, bei Zweifel fragen),
  **Monochrom**. Die Zuordnung „welche Datei, welcher Ordner bekommt welches Icon“, die Stilwahl in den Einstellungen
  und die Komponente sind eigener Code.
- **K6 — UI-Icons** (R1-Q6): **Lucide** (ISC) für die ganze Oberfläche, als Inline-SVG über eine
  `<Icon name>`-Komponente, eingecheckte Teilmenge. Der App Ring (heute Material Symbols) zieht später nach.
- **K7 — Reiter** (R1-Q9): Schwebende **Pillen** mit Rundung ringsum und Abstand zueinander, der aktive gefüllt.
- **K8 — Cursor** (R1-Q8, neu gefasst): Die Dauer des Gleitens wächst mit der Entfernung (ein Zeichen ≈ 80 ms, ein
  Sprung über den halben Bildschirm ≈ 200–250 ms, weiches Auslaufen). Die Spur wird eine zusammenhängende **Schliere**,
  die sich zum Ziel hin zusammenzieht; das Leuchten ist im Flug stärker. Tippen und Pfeiltasten bleiben kurz und
  dezent; stark wirkt es bei Mausklicks und Sprüngen (`gg`, `G`, `gd`, ⌘P-Ziele). Einstellung „Cursor-Animation: aus
  / dezent / stark“, Vorgabe stark. Heute: pauschal 110 ms (`fileapp/cursorGlide.ts`).
- **K9 — UI-Skalierung „Auto“** (R2-Q1): Nach der **körperlichen** Größe. macOS liefert die Maße des Bildschirms in
  Millimetern (CoreGraphics `CGDisplayScreenSize`, über `axiomata-macos` wie die Schriften per FFI) und die Auflösung
  in Punkten. Bezug ist, wofür macOS ausgelegt ist: ein MacBook in Standardeinstellung (≈ 127 Punkte pro Zoll) für
  eingebaute Bildschirme, ein Schreibtischmonitor wie das Studio Display (≈ 110) für externe — er steht weiter weg
  (Nachtrag beim Bau: gegen 127 gerechnet käme ein nativ betriebener 40″-5K2K nur auf 1,1). Faktor = Punkte pro Zoll /
  Bezug, begrenzt auf 1,0–1,6, in Schritten von 0,05. Wechselt das Fenster den Bildschirm, wird neu gerechnet. Manuell
  übersteuerbar: „UI-Größe: Auto / 90 %–150 %“.
- **K10 — Sofort app-weit** (R2-Q2): Der Faktor `--ax-ui-scale` multipliziert alle `--ax-font-size-*`,
  Icon-Größen und Abstände der UI, und das gilt vom Fundament an in der ganzen App. **Nachtrag beim Bau (Owner,
  nach dem Screenshot-Durchgang):** auch die Geometrie wächst mit — Kacheln, schwebende Panels und die Breite des
  Dateibaums werden in unskalierten Einheiten gespeichert und mal Faktor gezeichnet, feste Dialog- und Panelbreiten
  sind `calc(N px × Faktor)`. Die Alternative, die ganze Webansicht zu zoomen (Tauri `set_zoom`), hat der Owner
  verworfen: „fühlt sich nach Browser an“. Dazu gehört ein Durchgang mit
  Screenshots durch alle Module (21:9 und MacBook), um Stellen mit festen Pixelmaßen zu finden. Ausgenommen sind die
  Editor-Schrift und der Code im FilePeek.
- **K11 — Schrifttreppe und Mindestgrößen** (R2-Q3): xs 12, sm 13, base 14, lg 16, xl 20 px, je mal Faktor
  (heute 11/12/14/16/20). Lesetext nie unter `sm`, `xs` nur für Marken und Zähler. Klickbare Knöpfe haben eine
  Mindestgröße als Token (≈ 28 px mal Faktor), mit Icon — erledigt auch den Knopf „groß öffnen“ der Kanban-Kachel.
- **K12 — Gerenderte Ansichten** (R2-Q4): Die Markdown-Ansicht (FilePeek, Mail-Zusammenfassung, Orbit,
  Datei-Panel „zum Lesen“) ist UI-Text, hat heute keine eigene Größe und erbt die kleine Umgebung. Künftig liest sie
  in `base` mal Faktor.
- **K14 — Farbige Reiter** (Owner-Nachtrag 2026-09-28): Jede Sprache bzw. Dateiendung hat eine Farbe (Rust orange,
  TypeScript blau, Python gelb, Markdown grau …) aus *einer* Tabelle, die LK4 auch für die Datei-Icons nutzt. Die Pille
  trägt einen dezenten Farbton davon, die aktive einen kräftigeren; abschaltbar in den Einstellungen. Die Farben gehen
  durch `--ax-*`-Tokens, damit jedes Theme sie anpassen kann.
- **K13 — Einstellung „UI-Größe“** (R2-Q6): in den allgemeinen App-Einstellungen neben dem Theme; der Inspector der
  Datei-App verweist darauf.

## 3. Checkpoints (Vorschlag)

- **LK0 — Fundament**: `axiomata-macos::display` (Punkte pro Zoll je Bildschirm), Tauri-Befehl + Ereignis beim
  Bildschirmwechsel, `--ax-ui-scale` (K9/K10), neue Schrifttreppe und Mindestgrößen (K11), Rundungs- und
  Icon-Größen-Tokens, `<Icon>` mit der Lucide-Teilmenge (K6), die Einstellung „UI-Größe“ (K13), die Markdown-Ansicht
  auf `base` (K12); Screenshot-Durchgang durch alle Module.
- **LK1 — Inspector** (K2, K3): Settings | Shortcuts, die Kürzel-Liste aus den Keymaps, Minimap bleibt.
- **LK2 — Split** (K4): das Dock-Modell der IDE in der Datei-App.
- **LK3 — Reiter und Knöpfe** (K6, K7, K14): farbige Pillen und Icon-Knöpfe in der Datei-App und der IDE; die
  Farbtabelle je Sprache entsteht hier.
- **LK4 — Datei-Icons** (K5): vier Sätze, Zuordnung, Stilwahl.
- **LK5 — Cursor** (K8).

Danach: die Optik je Modul in den Rest der App übertragen, jedes Modul eigens geplant.

## 4. Offen beim Bau

- Welche Icon-Themes Zed anbietet und was dort „Git“ heißt (K5).
- Der Screenshot-Durchgang in LK0 kann Module zeigen, deren feste Pixelmaße den Faktor nicht vertragen — sie werden
  im Durchgang gelistet und dem Owner vorgelegt, nicht stillschweigend umgebaut.

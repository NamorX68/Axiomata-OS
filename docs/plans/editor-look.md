# Editor: drei Erweiterungen und eine moderne Optik — der künftige Standard für Axiomata

Stand: 2026-09-28, gegrillt (Runden 1–2), vom Owner bestätigt und umgesetzt (LK0–LK5). Live-Test in der App
bestanden (2026-09-30, Owner): Reiter, Baum-Icons, Minimap, Cursor-Flug und Inline-Diagnose sind ok; Glow, Spur und
Tab-Tönung auf Owner-Wunsch zweimal verstärkt. Offen: die Übertragung der Optik auf die übrigen Module, jedes eigens geplant. Anlass (Owner): „Zur Zeit sieht das alles nach einer
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

**LK0 umgesetzt (2026-09-28, `085d7d6`)** — UI-Skalierung, Schrifttreppe, Icon-System; eingeschoben `3f4be8a`: Kacheln
auch von links / links unten ziehbar (Owner-Wunsch).

**LK1 umgesetzt (2026-09-28):** `fileapp/Inspector.svelte` als eigene Spalte rechts der Reiter in der Datei-App (nicht
mehr pro Editor und nicht mehr über dem Text — die Minimap bleibt sichtbar), Reiter Settings | Shortcuts, geöffnet über
zwei Icon-Knöpfe im Kopf (Regler, Tastatur) statt „⚙“. `fileapp/shortcuts.ts` ist der Katalog; `shortcuts.test.ts`
drückt jeden Eintrag der geprüften Gruppen durch die echte Keymap (jeder Eintrag muss wirken, jede Wirkung der Keymap
muss aufgeführt sein) und gleicht die Vi-Gruppen mit `VI_GRAMMAR` (`editor/vi/parse.ts`) und `EX_COMMAND_NAMES`
(`editor/vi/ex.ts`) ab. Tasten außerhalb der Keymap (Datei-App, Sprachserver, Menüs) sind von Hand gepflegt.
`EditorSettingsPanel` ist nur noch Inhalt.

**LK2 umgesetzt (2026-09-28):** Die Datei-App legt ihre Reiter in Gruppen nebeneinander oder untereinander, auf dem
Dock-Baum der IDE (`ide/layout.ts`); die Datei-Regeln stehen in `fileapp/fileDock.ts` (eine Datei höchstens einmal im
ganzen Layout — Owner-Entscheid beim Bau, s. u.; der Vorschau-Reiter je Gruppe; die fokussierte Gruppe für neue Reiter,
⌃⇥ und ⌘1–9; gespeichert unter `settings.editor.dock`, alte `settings.editor.tabs` werden einmal als eine Gruppe
übernommen). ⌘\ schiebt den sichtbaren Reiter in eine neue Gruppe rechts, ⇧⌘\ darunter (nach der Tastenposition,
weil ein deutsches Layout „\“ mit ⌥⇧7 tippt); ein Reiter lässt sich in eine Gruppe (Mitte) oder an einen Rand ziehen,
Teiler mit der Maus verschieben — Ziehgeometrie aus `ide/dock.ts`. Die Editoren werden einmal flach gezeichnet und in
ihre Gruppe umgehängt (`ide/paneStore.ts`): ein Reiterumzug behält Cursor, Undo und ungespeicherten Text (im Browser
geprüft). Neu: `FileDockNode.svelte`, `FileGroup.svelte`, `fileDockContext.ts`; `tabs.ts` liest nur noch alte Stände.

**LK3 umgesetzt (2026-09-28):** Reiter als schwebende Pillen in Datei-App und IDE (`FileGroup.svelte`,
`ide/PaneGroup.svelte`), jede mit dem Farbton ihrer Sprache (K14): `core/languageColors.ts` (Sprache → Token,
TSX = TypeScript), Tokens `--ax-lang-*` in `themes/tokens.css`, für eigene Themes freigegeben (`theme/validator.ts`);
abschaltbar („Tabs coloured by language“, `tabColors`). Stärke zweimal erhöht (32 % / hover 40 % / aktiv 50 %, Rand 75 %), die
fokussierte Gruppe trägt den Akzentrand. Icon-Knöpfe statt Text: Datei-App-Kopf (öffnen, zuletzt, Einstellungen,
Kürzel, zurück zum OS), Baumkopf (Dateien | Suche, versteckte Dateien, neu lesen), IDE-Kopf (Terminal, zurück),
IDE-Files-Pane, × und + in den Tab-Leisten. Drei Lucide-Icons nachgeladen (history, layout-grid, eye-off).

**LK4 umgesetzt (2026-09-28):** Icons im Dateibaum (Datei-App und IDE-Files-Pane) in vier Stilen, wählbar in den
Editor-Einstellungen („File icons“, Vorgabe Catppuccin, auch „None“). `scripts/vendor-file-icons.sh` holt je Dateiart ein
Icon aus Catppuccin (`@iconify-json/catppuccin`, MIT), Octicons für „Git“ (`@iconify-json/octicon`, MIT) und JetBrains'
Expui-Icons (hell und dunkel, `intellij-community` am festen Commit, Apache 2.0) nach `src/ui/icons/fileIcons.ts`, mit
Lizenztexten; Monochrom nimmt Lucide. `core/fileIcons.ts` bestimmt die Dateiart (ganzer Name → Endung → Sprache aus der
Farbtabelle) und wählt JetBrains' Variante nach `--ax-color-scheme`. JetBrains hat im Community-Repository keine Icons für
Rust, Python, Svelte und Lua — dort steht sein allgemeines Datei-Icon. Die Aufklapp-Pfeile sind Lucide-Chevrons.

**LK5 umgesetzt (2026-09-28), verstärkt auf Owner-Wunsch (2026-09-29):** Die Cursor-Animation (`fileapp/cursorGlide.ts`, Logik in `editor/decorations.ts`)
richtet ihre Dauer nach der Entfernung (`glideMotion`: stark 90 → 380 ms, dezent 70 → 130 ms, Wurzel-Kurve bis 700 px),
und bei „stark“ folgt ein Schwanz 3,6-mal langsamer als der Kopf: zwischen beiden liegt eine Schliere (konvexe Hülle von
Schwanz- und Kopf-Kasten, mindestens 14 px breit, 90 % deckend, Verlauf zum Schwanz hin durchsichtig), die sich beim Landen ins Ziel
zusammenzieht; das Leuchten im Flug ist stärker (34 statt 12 px). Tippen und Pfeile bleiben durch die kurze Strecke kurz.
Einstellung „Cursor: Strong / Subtle / Off“, Vorgabe Strong; gespeicherte „trail“/„glide“ werden übernommen. Im Browser
(verlangsamt) mitten im Flug geprüft.

**K4 präzisiert (Owner, beim Bau von LK2):** Dieselbe Datei in zwei Gruppen hätte einen gemeinsamen Cursor, weil die
Auswahl im Dokument liegt, nicht in der Ansicht. Entscheid: jede Datei nur einmal. „Dieselbe Datei zweimal mit eigenem
Cursor“ ist vorgemerkt — es braucht Auswahl, Mehrfach-Cursor, Undo-Auswahl und Vi-Zustand pro Ansicht.

## 5. Die Optik in der IDE (gegrillt 2026-09-28, I1–I7)

Erstes Modul, auf das der Standard übertragen wird (Owner: „fangen wir mit der IDE an“; das Kanban folgt, eigens geplant).

- **I1 — Gruppen bündig wie in Zed**, in IDE und Datei-App: keine schwebenden, gerahmten Kästen mehr, sondern
  aneinanderstoßende Gruppen mit 1-px-Trennlinie (zum Ziehen breiter).
- **I2 — Icon-Leiste im Agenten-Pane**: statt gedrehter Wörter senkrechte Icons (Terminal, Plan, Diffs, Inbox) mit
  Tooltip; offene Diffs und ungelesene Inbox-Einträge als Abzeichen.
- **I3 — Statuszeile des Agenten**: Zustand als Punkt + Wort, Name, Harness, Worktree, Port, Befehl gedämpft;
  „New session“ und „Restart“ als Icon-Knöpfe.
- **I4 — Agenten- und Projekt-Menü als Zeilen**: Punkt, Name, darunter gedämpft Harness/Modell; Bearbeiten und Entfernen
  als Icon-Knöpfe beim Überfahren; „New agent…“/„New project…“ als Zeile mit Plus; die Befehlszeile nur im Dialog.
- **I5 — Kopfzeile**: der Hinweis „Drag a tab…“ entfällt (steht in der Kürzel-Liste); die IDE bekommt denselben
  Inspector (Settings | Shortcuts) wie die Datei-App, die Kürzel-Liste einen IDE-Abschnitt.
- **I6 — Diff und Git-Dialoge**: Hauptaktionen (Commit, Übernehmen, Verwerfen) bleiben Text, als einheitliche Pillen —
  primär Akzent, sekundär neutral, gefährlich rot; kleine Aktionen (Hunk verwerfen, neu laden, Datei öffnen) als Icons.
- **I7 — Agenten-Reiter** getönt nach Harness (Claude Code, Opencode), der Punkt bleibt der Zustand.

Checkpoints: **IK1** Gruppen bündig (I1), Kopfzeile + Inspector (I5), Menüs (I4); **IK2** Agenten-Pane (I2, I3),
Reiter-Tönung (I7), Diff und Dialoge (I6).

**IK1 + IK2 umgesetzt (2026-09-28):** alle Punkte I1–I7. Globale Pillen-Knöpfe `.ax-btn` (`primary`, `danger`) in
`styles.css` als Standard für Text-Knöpfe; `fileapp/inspectorSurface.svelte.ts` teilen Datei-App und IDE; Harness-Farben
`--ax-harness-*`. **Bekannte Einschränkung (Review):** das Diffs-Abzeichen im Agenten-Pane kommt aus der Diff-Ansicht, die
erst beim ersten Öffnen des Reiters entsteht und nur abfragt, solange sie sichtbar ist — vorher kein Abzeichen, danach
kann es veralten. Ein immer aktuelles Abzeichen bräuchte eine eigene Abfrage je Agent; offen zum Entscheiden.

## 6. Die Optik im Kanban (gegrillt 2026-09-28, B1–B7)

Zweites Modul nach der IDE. Heutiger Stand (Browser-Blick): flache Karten mit umrandeten Label-Chips, Fälligkeit und
Zuständiger als Mono-Text; Spalten mit Griff, Name und Zahl, beim Überfahren ein rohes Auswahlfeld (Rolle) und ×; das
Karten-Detail als Kasten über der mittleren Spalte; „+ Spalte“, „+ Karte“ und die Archiv-Checkbox als Text.

- **B1 — Karten** (Linear/Zed): abgerundet mit Haarlinie, beim Überfahren leicht, beim Ziehen deutlich abgehoben; Labels
  als gefüllte, zart getönte Chips (Farben weiter aus `--ax-label-*`); Fälligkeit mit Kalender-Icon — gedämpft, bald
  fällig gelb, überfällig rot; der Zuständige als kleines Abzeichen mit Bot-Icon (Agent) bzw. Personen-Icon (Owner).
- **B2 — Spalten** als abgerundete, leicht abgesetzte Bahnen mit Abstand; im Kopf ein Punkt für die Rolle (offen grau,
  in Arbeit Akzent, fertig grün), der Name, die Zahl als Pille; Rolle, Umbenennen und Entfernen hinter einem
  „…“-Menü statt Auswahlfeld und ×.
- **B3 — Knöpfe**: „+ Spalte“ als Plus-Icon im Kopf, „+ Karte“ als ruhige Zeile mit Plus am Spaltenende, Lupe im
  Filterfeld, Archiv als Icon-Umschalter, im Detail „Archivieren“/„Löschen“ als `ax-btn` (Löschen `danger`).
- **B4 — Karten-Detail als Seitenleiste rechts im großen Brett** (wie Linear): das Brett bleibt sichtbar, ein Klick auf
  eine andere Karte wechselt den Inhalt. In der kleinen Kachel öffnet ein Klick die Karte im großen Brett.
- **B5 — Brett-Kopf als eine Werkzeugleiste**: links der Brettname (Umschalter bei mehreren Brettern), Mitte das
  Filterfeld, rechts die Label-Filter-Chips, Archiv und „+ Spalte“ als Icons.
- **B6 — Farbstreifen links an der Karte** in der Farbe ihres ersten Labels; abschaltbar in den Kanban-Einstellungen.
- **B7 — Ein Kartenstil statt vier** (Owner): die Wahl „Auto / Flach / Kante / Schwebend“ (`kanbanPrefs.ts`
  `cardStyle`, `kanban-settings.svelte`, `data-cards`) fällt weg; es bleibt der Stil aus B1. Die Theme-Tokens
  `--ax-card-bg/border/shadow` bleiben, damit helle und dunkle Themes passen; ein gespeichertes `cardStyle` wird
  ignoriert.

Checkpoints (Vorschlag): **KB1** Karten + ein Stil + Farbstreifen (B1, B6, B7); **KB2** Spalten + Knöpfe + Kopfleiste
(B2, B3, B5); **KB3** Detail als Seitenleiste (B4).

**KB1–KB3 umgesetzt (2026-09-28):** Karten mit Haarlinie, Anheben beim Überfahren, gefüllten Label-Chips, Fälligkeit mit
Kalender-Icon (`dueState` kennt jetzt `soon`), Zuständigem als Abzeichen; Farbstreifen nach dem ersten Label (Schalter in
den Kanban-Einstellungen, `kanbanPrefs.cardStripes`); die vier Kartenstile sind weg. Spalten als Bahnen mit Rollen-Punkt
(als `data-role` — die Klasse `open` ist im Kanban schon der Kartenknopf), Zahl als Pille, „…“-Menü mit Rolle, Umbenennen,
Entfernen; „+ Karte“ als Zeile, „+ Spalte“ und Archiv als Icons, im großen Brett eine Werkzeugleiste mit Filterfeld und
Label-Chips. Das Karten-Detail ist im großen Brett eine Seitenleiste rechts (Esc schließt); ein Klick in der Kachel öffnet
das große Brett und reicht die Karte über einen modulweiten Store (`focusCard`) hinüber. Einzelkarten-Panels älterer
Stände (`config.cardId`) zeigen weiter dasselbe Detail.

## 4. Offen beim Bau

- Welche Icon-Themes Zed anbietet und was dort „Git“ heißt (K5).
- Der Screenshot-Durchgang in LK0 kann Module zeigen, deren feste Pixelmaße den Faktor nicht vertragen — sie werden
  im Durchgang gelistet und dem Owner vorgelegt, nicht stillschweigend umgebaut.

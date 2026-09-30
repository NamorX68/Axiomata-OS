# Grobplan: Projekt-Werkzeuge im Editor (Wurzeln, Outline, Git, Run, Debug)

Status: **gegrillt 2026-09-30 (Q1–Q19, Nachgrill Q20–Q27, bestätigt), **#47 ist gebaut (2026-09-30) — als Projekt neu/öffnen/schließen, nicht als Wurzel-Auswahl; der Rest weiter geparkt**.
Ursprünglich geparkt 2026-09-29 („keine Resourcen für Umsetzung“). Die Entscheidungen unten sind der
abgelegte Grill-Stand; vor dem Bauen jeden Punkt anhand dessen in Checkpoints zerlegen
(siehe die Arbeitsweise im Dachplan [`agentic-ide.md`](agentic-ide.md)).
Gehört zum Editor ([`editor.md`](editor.md), ED0–ED6 fertig) und zur IDE. Karten auf dem Kanban-Board #1:
#47–#51, alle mit den Labels `feature`, `editor`, `geparkt`.

## Anlass

Der Owner hat am 2026-09-29 beim Arbeiten im Datei-App sechs Dinge gemeldet, die ihm gegenüber Zed und VS Code fehlen
(Screenshots aus Zed: Git-Panel, Outline, Debugger):

1. **Fehler** — die erste Datei aus dem Baum blieb leer. **Behoben** am selben Tag (Karte #46): der Parken-Effekt
   in `fileapp/FileAppView.svelte` und `ide/IdeView.svelte` hing an `dockEl`/`storeEl` und parkte den Editor direkt nach
   dem Platzieren wieder im `pane-store`; er reagiert jetzt nur noch auf `dock`/`layout` (`untrack`).
2. Wurzeln im Dateibaum hinzufügen und entfernen — „irgendwie fehlt mir der Projektgedanke“.
3. Git-Änderungen als eigenes Panel.
4. Eine Outline-Ansicht.
5. Programme kompilieren und ausführen.
6. Ein Debug-Panel.

Punkt 1 ist erledigt; 2–6 sind unten. Bewusst als Grobplan: was es heute gibt, was fehlt, in welcher Reihenfolge es
sich lohnt, und was der Grill vom 2026-09-30 entschieden hat.

## Was schon da ist

- **Wurzeln:** `axiomata-files` kennt Wurzel-IDs `workspace`, `project:<id>`, `worktree:<agent>`, `grant:<id>`;
  neue Orte kommen nur über `file_pick` (nativer Dialog aus Rust, `grant:<id>`, `~/.axiomata/file-grants.json`).
  Das Wurzel-Menü im Baum (`fileapp/FileTree.svelte`) hat nur „New file“ und „New folder“.
- **Git:** nur für Agent-Worktrees (`crates/axiomata-ide/src/git.rs`, `git` als Unterprozess, nie ein Push):
  Diff gegen den Basis-Branch, Discard je Datei/Hunk, Commit, Take-over; Diffs-Tab und Datei-/Agent-Diff-Panes
  (M7.3, `git-layer.md`). Für einen normalen Projektordner fehlen Status, Stage/Unstage, History und Branch.
- **Symbole:** der LSP-Client (`axiomata-files::lsp`, `src/editor/lsp/`) spricht completion, hover, definition,
  implementation, typeDefinition, references, rename, codeAction, signatureHelp, formatting — **kein**
  `documentSymbol`. tree-sitter-Grammatiken sind gebaut und eingecheckt (`public/grammars/`).
- **Ausführen:** nichts. Die IDE hat Terminals (PTY) und Agenten.
- **Debuggen:** nichts, kein DAP-Client.

## Reihenfolge (Vorschlag)

| # | Punkt | Karte | Größe | Warum hier |
|---|---|---|---|---|
| 1 | Wurzeln hinzufügen/entfernen | #47 | klein | Der Projektgedanke bestimmt, worauf die anderen Panels sich beziehen |
| 2 | Outline | #49 | klein–mittel | Liefert auch Breadcrumbs und bessere Sticky Scroll |
| 3 | Git-Panel | #48 | mittel | Engine und Diff-Ansicht großenteils da |
| 4 | Run/Tasks | #50 | mittel | Braucht ein Ausgabe-Pane und Konfiguration |
| 5 | Debug | #51 | groß | Baut auf Run auf, braucht einen DAP-Client |

## Gegrillte Entscheidungen (2026-09-30, Q1–Q19, bestätigt)

Rahmen: alle fünf Punkte (#47–#51) in einer Runde gegrillt, nichts wird gebaut. ED7-Herauslösung
(#35), Snippets/Completion (#36) und ein eigener Agent für die Codebasis bleiben draussen.

- **Wurzeln (#47):** eigener globaler Arbeitsbereich = gemerkte Auswahl von Wurzeln
  (`workspace`, `project:`, `grant:`) in `settings.editor`, getrennt von IDE-Projekten
  (die behalten Layout, Agenten, Worktrees). Auswahl im Baumkopf, global (nicht pro Fenster).
  Hinzufügen nur über den nativen Dialog (`grant:<id>`, nie ein Pfad aus der Webview — D5-Regel).
  Entfernen heißt immer nur: aus der Ansicht nehmen (ausblenden / aus der Liste) — nie wird ein
  Ordner auf der Platte gelöscht oder widerrufen-im-Sinne-von-löschen. Echtes Löschen von Dateien
  ist die separate Baum-Aktion (ED4.4/W13), nicht Teil von #47. Ausgeblendete Wurzeln lassen offene
  Tabs mit ungespeichertem Stand stehen; erneutes Hinzufügen holt nur die Sicht zurück.
- **Outline (#49):** tree-sitter trägt die Struktur (dieselbe Quelle wie Faltung/Textobjekte/Sticky
  Scroll), LSP-`documentSymbol` verfeinert nur Namen/Bereiche, sobald ein Server läuft. Darstellung:
  Spalte neben dem Baum (Datei-App) bzw. Dock-Pane (IDE), eingeklappter Baum, Suchfeld, Pin,
  Klick springt, Breadcrumbs über dem Editor aus denselben Daten. Der Baum folgt dem Cursor
  (aktuelles Symbol hervorgehoben), Pin friert ein.
- **Git (#48):** Engine in `axiomata-files` (`git` als Unterprozess, nie ein Push — M7.3-Regel),
  ein Panel-Bauteil für Datei-App und IDE (wie `ProjectSearch.svelte`), ein Panel mit Repo-Wähler
  bei mehreren Wurzeln. Erster Wurf nur Changes: Status, Stage/Unstage je Datei und je Hunk
  (Diff-Ansicht aus CP8/CP9 wiederverwendet), „Stage All“, Commit-Nachricht, Branch + Fetch.
  History (Log + Diff je Commit, Branch-Liste) ist ein eigener späterer Punkt.
- **Run (#50):** generisch für Rust + Python + TS/Node. Konfiguration als Datei im Projekt
  (Zed-Vorbild `tasks.json`) + Auto-Erkennung (`cargo build/test/run`, `npm run …`,
  `uv run …`/`pytest`), Persönliches unter `~/.axiomata`. Ausführung auf der Terminal-PTY-Engine,
  Ausgabe in einem Dock-Pane mit klickbaren `Datei:Zeile`-Fehlern und Problem-Matchern je Sprache (Diagnosen im Editor, Nachgrill), Umgebung wie `toolenv.rs`,
  Abbrechen/Neustart dabei.
- **Debug (#51):** Rust zuerst, Python + TS/Node als spätere Checkpoints (Nachgrill Q20–Q27). DAP-Client in Rust analog
  `axiomata-files::lsp`, nur gesprochene Methoden. Adapter-Tabelle eingebaut (`codelldb`/`lldb-dap`,
  `debugpy`, Node-Inspector), Suchpfad wie bei LSP (`PATH`, Homebrew, `mason/bin` zuletzt).
  `debug.json` im Projekt (Zed-Vorbild). Breakpoints in der Gutter-Spalte (gemerkt je Datei),
  Panel mit Breakpoint-Liste („Neue Sitzung“), Variablen, Call-Stack, Konsole.
- **Trust-Modell (Run/Debug):** `tasks.json`/`debug.json` im Projekt führen Befehle/Adapter aus,
  also fremden Code aus einem Repo. Gleicher Rahmen wie `lsp.json` für Adapter- und Sprachtabellen: nur eingebaute Tabelle +
  Hand-Override in `~/.axiomata/*.json` (kein Tauri-Schreibbefehl), nur gesprochene Methoden, vor
  dem Start anzeigen, was läuft. Die Projektdatei selbst läuft erst nach Hash-Bestätigung (Nachgrill).

## Nachgrill (2026-09-30, Q20–Q27)

Zweite Runde über denselben Plan, Fokus auf Widersprüche und Zuschnitt. Weiter **kein Bau**.

- **Trust präzisiert (ersetzt die Lesart „nur Tabelle + Override“ für Projektdateien):** eine
  `tasks.json`/`debug.json` im Projekt wird gelesen, läuft aber erst nach einer Bestätigung — beim ersten
  Lauf und nach jeder Änderung (Datei-Hash) zeigt ein Dialog die Befehle/Adapter-Pfade/Argumente. Die
  Freigabe (Hash je Datei) liegt unter `~/.axiomata`, geschrieben aus Rust, kein Tauri-Schreibbefehl aus
  der Webview. Gilt für `debug.json` genauso. Adapter- und LSP-Tabellen bleiben eingebaut + Hand-Override.
- **Zwei Projektbegriffe bleiben getrennt:** IDE-Projekt (Layout, Agenten, Worktrees) und globaler
  Editor-Arbeitsbereich (Wurzelauswahl, #47) — bewusst, wie gegrillt.
- **Git: gemeinsamer Unterbau.** Prozessaufruf, Status- und Diff-Parsing ziehen nach `axiomata-files`;
  `axiomata-ide::git` behält nur Worktree/Basis-Branch/Take-over und setzt darauf auf. Dieser Umbau ist
  der **erste Checkpoint von #48** (M7.3-Tests müssen grün bleiben).
- **ED7 nimmt die Werkzeuge mit.** Jedes Werkzeug (Outline, Git, Run, Debug) wird ohne `axiomata-core`
  gebaut (nur `axiomata-files`/`axiomata-terminal`), damit ED7 nur noch eine Hülle ist.
- **Run (#50): Problem-Matcher je Sprache mit Diagnosen im Editor** (Wellen/Markierungen wie bei LSP-
  Diagnosen, Quelle „task“), nicht nur klickbare Stellen. Je Toolchain ein Matcher (rustc/cargo
  `--message-format=json`, tsc, pytest/ruff o. ä.); der Zuschnitt des ersten Wurfs wird im Checkpoint-
  Grill von #50 festgelegt. **Offen:** wann Diagnosen einer Task verschwinden (nächster Lauf? Datei-Edit?).
- **Debug (#51): ein Adapter zuerst.** DAP-Client bleibt generisch; Abnahme mit `lldb-dap`/`codelldb`
  (Rust), Python (`debugpy`) und Node folgen je als kleiner Checkpoint. Ändert „alle drei von Anfang an“ oben.
- **Parkung bleibt**, bis ein konkreter Schmerz auftritt (Owner). Reihenfolge zu M7.4–M7.6/ED7 daher
  nicht festgelegt; der Vorschlag der Tabelle oben gilt, wenn gezogen wird.

## #47 umgebaut und gebaut: Projekt neu / öffnen / schließen (Owner, 2026-09-30)

Der Owner hat die Wurzel-Auswahl durch einen schlichteren Projektbegriff ersetzt (ersetzt „Wurzeln (#47)“ und die
Zeile „Zwei Projektbegriffe bleiben getrennt“ des Nachgrills):

- **Ein Register für Editor und IDE:** die Tabelle `projects` (M7.1). Ein Projekt ist ein Ordner mit einer Zeile dort;
  **keine Projektdatei im Ordner** (Repo bleibt sauber, Worktrees bleiben unberührt). Der Ordner ist die Wurzel
  `project:<id>`.
- **Öffnen:** Ordner über den nativen Dialog aus Rust (`project_open`), gibt es ihn schon als Projekt, wird dieses
  geöffnet, sonst angelegt (Name = Ordnername; `axiomata_ide::store::open_root`). `$HOME` und darüber bleiben
  abgelehnt (L0).
- **Neu:** Name + „git init“ (Vorgabe an), dann Dialog für den Elternordner (`project_new`;
  `axiomata_ide::newproject`, legt den Ordner an, nie einen bestehenden, räumt bei einem Fehler auf).
- **Schließen:** nimmt das Projekt nur aus dem Baum. Tabs (auch mit ungespeichertem Stand), Ordner und Registerzeile
  bleiben.
- **Ein Projekt je Ansicht:** Editor und IDE halten je eines offen (Editor: `settings.editor.tree.project`). Der Baum,
  ⌘P und die Projektsuche zeigen nur dessen Ordner. Der Second-Brain-Workspace bleibt **nicht** dauerhaft im Baum
  (der Editor soll ohne Vault denkbar sein); er lässt sich als Ordner öffnen. Eine einzeln geöffnete Datei ist in
  keinem Projekt.
- **Mehrere Ordner (VS-Code-Workspace)** sind ausdrücklich später; dann kommt „Wurzeln hinzufügen/ausblenden“ zurück.
- **Bewusst entfernt:** die Tauri-Befehle `create_ide_project` und `set_ide_project_root` (nahmen einen Pfad aus der
  Webview). „Pfad ändern" läuft jetzt wie Öffnen über den Dialog (`project_set_root`). Im Editor gibt es noch keinen Weg,
  ein Projekt aus dem Register zu nehmen (nur in der IDE).

## 1. Wurzeln hinzufügen und entfernen (#47)

- **Hinzufügen:** Eintrag „Ordner hinzufügen“ (Dialog wie `file_pick`, Ergebnis ein `grant:<id>`). **Nie** einen Pfad
  aus der Webview annehmen — das ist die Grundregel von `axiomata-files`.
- **Entfernen:** „Aus der Ansicht entfernen“ je Wurzel — **immer nur ausblenden**, niemals den Ordner löschen und
  niemals den Grant widerrufen. Eine ausgeblendete Wurzel lässt offene Tabs mit ungespeichertem Stand stehen; das
  erneute Hinzufügen holt nur die Sicht zurück.
- **Projektgedanke:** eine Auswahl von Wurzeln als Arbeitsbereich des Editors, gemerkt in den Editor-Einstellungen
  (`settings.editor`).
- **Entschieden (Grill 2026-09-30):** ein eigener globaler Arbeitsbereich, getrennt von den IDE-Projekten
  (die behalten Layout, Agenten, Worktrees); „Entfernen” ist immer nur ausblenden. → „Wurzeln (#47)” oben.

## 2. Outline (#49)

- **Zwei Quellen, die sich ergänzen:** LSP `textDocument/documentSymbol` (genau, braucht einen Server) und tree-sitter
  (offline; die Symbol-Abfragen kommen je Sprache mit der Grammatik, wie die Textobjekte in
  `editor/syntax/objects.ts`).
- **Darstellung:** eingeklappter Baum (Konstanten, Enums, Structs, `impl`-Blöcke, Funktionen), Suchfeld, „Pin“,
  Klick springt zur Stelle; im Datei-App als Spalte neben dem Baum, in der IDE als Pane.
- **Entschieden (Grill 2026-09-30):** tree-sitter zuerst und tragend, LSP-`documentSymbol` verfeinert nur; der Baum
  folgt dem Cursor, Pin friert ein, Breadcrumbs kommen im selben Zug. → „Outline (#49)" oben.
- **Regel:** Die Editor-Engine (`src/editor/`) importiert nichts aus der App; die Ansicht lebt in `src/fileapp/`.

## 3. Git-Panel (#48)

- **Umfang wie Zed:** Tabs *Changes* und *History*, Stage/Unstage (auch „Stage All“), Commit-Nachricht, aktueller Branch,
  Fetch. Diff-Ansicht auf dem Editor (CP8/CP9) wiederverwenden.
- **Engine:** neben `axiomata-ide::git` eine Engine für einen gewöhnlichen Projektordner (Status, Index, Log, Branches)
  — wieder `git` als Unterprozess, nie ein Push (Regel aus M7.3).
- **Entschieden (Grill 2026-09-30):** Engine in `axiomata-files`, ein gemeinsames Panel-Bauteil für Datei-App und
  IDE, Repo-Wähler bei mehreren Wurzeln; History ist **nicht** im ersten Wurf. → „Git (#48)" oben.

## 4. Kompilieren und Ausführen (#50)

- **Aufgaben je Projekt**, etwa `cargo build/test/run`, `npm run …`; Erkennung aus `Cargo.toml`/`package.json`,
  dazu eine Konfigurationsdatei (Vorbild: Zeds `tasks.json`).
- **Ausgabe** in einem Pane mit klickbaren Fehlern (Datei:Zeile) und Diagnosen aus der Ausgabe.
- **Entschieden (Grill 2026-09-30):** Konfiguration als `tasks.json` im Projekt plus Persönliches unter
  `~/.axiomata`; Ausführung auf der Terminal-PTY-Engine; Umgebung wie `toolenv.rs`; Abbrechen und Neustarten
  während des Laufs. → „Run (#50)" oben.

## 5. Debug-Panel (#51)

- **DAP-Client in Rust** (analog zu `axiomata-files::lsp`): feste Tabelle der Adapter (z. B. `codelldb` oder `lldb-dap`
  für Rust), nur Methoden, die der Client spricht; Sitzung starten, anhalten, schrittweise, Variablen, Call-Stack,
  Konsole.
- **Editor:** Breakpoints in der Gutter-Spalte, Anzeige der aktuellen Zeile; Panel mit Breakpoint-Liste und „Neue
  Sitzung“, `debug.json` (Vorbild Zed).
- **Entschieden (Grill 2026-09-30, Nachgrill: ein Adapter zuerst):** Rust (`lldb-dap`/`codelldb`) zuerst, Python und
  TS/Node danach als kleine Checkpoints; Adapter nur aus der eingebauten Tabelle, Hand-Override unter `~/.axiomata`;
  Trust wie bei `lsp.json`, Projektdateien mit Hash-Bestätigung. → „Debug (#51)" und „Trust-Modell" oben.

## Nicht in diesem Plan

Ein eigener Agent für die Codebasis, Snippets/Completion-Erweiterungen (L9 in `editor.md`) und die Herauslösung des
Editors (ED7) — eigene Karten (#36, #35). Für ED7 gilt aber: die Werkzeuge hier werden ohne `axiomata-core` gebaut
(siehe Nachgrill).

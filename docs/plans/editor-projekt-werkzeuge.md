# Grobplan: Projekt-Werkzeuge im Editor (Wurzeln, Outline, Git, Run, Debug)

Status: **geparkt** (Owner, 2026-09-29: „Zur Zeit haben wir nicht die Resourcen das umzusetzen“). Nur ein Grobplan —
nichts davon ist gegrillt oder begonnen. Vor dem Bauen jeden Punkt einzeln durchsprechen und bestätigen lassen
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
sich lohnt, und die Fragen, die vor jedem Punkt zu klären sind.

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

## 1. Wurzeln hinzufügen und entfernen (#47)

- **Hinzufügen:** Eintrag „Ordner hinzufügen“ (Dialog wie `file_pick`, Ergebnis ein `grant:<id>`). **Nie** einen Pfad
  aus der Webview annehmen — das ist die Grundregel von `axiomata-files`.
- **Entfernen:** „Aus der Ansicht entfernen“ je Wurzel: Grant widerrufen oder die Wurzel ausblenden; niemals den Ordner
  löschen. Für `workspace`, `project:` und `worktree:` nur ausblenden.
- **Projektgedanke:** eine Auswahl von Wurzeln als Arbeitsbereich des Editors, gemerkt in den Editor-Einstellungen
  (`settings.editor`).
- **Zu klären:** Verhältnis zu den IDE-Projekten (`project:<id>`) und zum Second-Brain-Workspace; ob ein
  „Projekt“ im Datei-App dasselbe ist wie in der IDE; ob die Auswahl pro Fenster oder global gilt.

## 2. Outline (#49)

- **Zwei Quellen, die sich ergänzen:** LSP `textDocument/documentSymbol` (genau, braucht einen Server) und tree-sitter
  (offline; die Symbol-Abfragen kommen je Sprache mit der Grammatik, wie die Textobjekte in
  `editor/syntax/objects.ts`).
- **Darstellung:** eingeklappter Baum (Konstanten, Enums, Structs, `impl`-Blöcke, Funktionen), Suchfeld, „Pin“,
  Klick springt zur Stelle; im Datei-App als Spalte neben dem Baum, in der IDE als Pane.
- **Zu klären:** LSP zuerst oder tree-sitter zuerst; ob der Baum dem Cursor folgt (aktuelles Symbol hervorheben);
  Breadcrumbs im selben Zug.
- **Regel:** Die Editor-Engine (`src/editor/`) importiert nichts aus der App; die Ansicht lebt in `src/fileapp/`.

## 3. Git-Panel (#48)

- **Umfang wie Zed:** Tabs *Changes* und *History*, Stage/Unstage (auch „Stage All“), Commit-Nachricht, aktueller Branch,
  Fetch. Diff-Ansicht auf dem Editor (CP8/CP9) wiederverwenden.
- **Engine:** neben `axiomata-ide::git` eine Engine für einen gewöhnlichen Projektordner (Status, Index, Log, Branches)
  — wieder `git` als Unterprozess, nie ein Push (Regel aus M7.3).
- **Zu klären:** eigenes Crate oder in `axiomata-files`; ein Panel für Datei-App und IDE oder zwei; ob History mit
  Diff je Commit im ersten Wurf dabei ist; Umgang mit mehreren Wurzeln (ein Panel je Repository).

## 4. Kompilieren und Ausführen (#50)

- **Aufgaben je Projekt**, etwa `cargo build/test/run`, `npm run …`; Erkennung aus `Cargo.toml`/`package.json`,
  dazu eine Konfigurationsdatei (Vorbild: Zeds `tasks.json`).
- **Ausgabe** in einem Pane mit klickbaren Fehlern (Datei:Zeile) und Diagnosen aus der Ausgabe.
- **Zu klären:** wo die Konfiguration liegt (im Projekt oder unter `~/.axiomata`); Verhältnis zum Terminal-Tile
  (dieselbe PTY-Engine oder ein eigener Prozess mit erfasster Ausgabe); Umgebung wie `toolenv.rs`; Abbrechen und
  Neustarten.

## 5. Debug-Panel (#51)

- **DAP-Client in Rust** (analog zu `axiomata-files::lsp`): feste Tabelle der Adapter (z. B. `codelldb` oder `lldb-dap`
  für Rust), nur Methoden, die der Client spricht; Sitzung starten, anhalten, schrittweise, Variablen, Call-Stack,
  Konsole.
- **Editor:** Breakpoints in der Gutter-Spalte, Anzeige der aktuellen Zeile; Panel mit Breakpoint-Liste und „Neue
  Sitzung“, `debug.json` (Vorbild Zed).
- **Zu klären:** welche Sprachen zuerst (Rust, weil der Owner Rust schreibt); wie Adapter installiert werden
  (wie die Language Server: nur eine eingebaute Tabelle oder `~/.axiomata`); Sicherheit — ein Adapter führt fremden
  Code aus, also derselbe Rahmen wie bei `lsp.json`.

## Nicht in diesem Plan

Ein eigener Agent für die Codebasis, Snippets/Completion-Erweiterungen (L9 in `editor.md`) und die Herauslösung des
Editors (ED7) — eigene Karten (#36, #35).

# Grobplan: Projekt-Werkzeuge im Editor (Wurzeln, Outline, Git, Run, Debug)

Status: **gegrillt 2026-09-30 (Q1–Q19, Nachgrill Q20–Q27, bestätigt), **#47 (Projekt neu/öffnen/schließen), #49 (Outline) und #48 (Git-Panel, erster Wurf) sind gebaut (2026-09-30/10-01); der Rest weiter geparkt**.
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
| 5 | Debug | #51 | groß | Baut auf Run auf, braucht einen DAP-Client — **Python/debugpy gebaut (2026-10-02)**, Rust/Node folgen |

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
  Webview). „Pfad ändern" läuft jetzt wie Öffnen über den Dialog (`project_set_root`). Die Projektleiste des Editors bietet
  dasselbe wie die IDE-Auswahl: Ordner ändern und „aus der Liste entfernen“ (nie den Ordner).

## #49 Outline gebaut (2026-09-30)

Drei Checkpoints, wie im Gespräch festgelegt (Owner: Bereich unter dem Baum, Code + Markdown, LSP später):

- **CP1 Symbole:** `editor/syntax/outline.ts` — `outlineFromTree` (Knotentabelle wie bei Textobjekten/Faltung: Rust, TS/JS/TSX,
  Python, Swift, Lua, Bash), `outlineFromMarkdown` (Überschriften, nicht in Code-Zäunen), `pathAt`, `filterOutline`.
  Nichts aus Funktionskörpern (Closures, lokale Helfer); `const f = () => …` zählt als Funktion; Funktionen in
  Klassen/impl/trait sind `method`; höchstens 5000 Symbole; Namen aus dem `TextStore`, nie `node.text`.
- **CP2 Ansicht:** `fileapp/OutlinePanel.svelte` als einklappbarer Bereich unter dem Dateibaum (Höhe ziehbar, in
  `settings.editor.tree`), Filterfeld, Klick springt zum Namen, die Zeile am Cursor ist markiert und im Bild gehalten,
  Pin friert die Markierung ein. `FileEditor` meldet (`onOutline`, Symbole 250 ms nach der letzten Änderung, der Cursor
  sofort) über `FileTab` an die Ansicht, die die Meldung des vorderen Tabs zeigt. Keine Outline im Light-Modus.
- **CP3 Breadcrumbs:** in der Kopfzeile hinter dem Dateipfad: die Symbole, die den Cursor halten, klickbar.
- **Offen:** LSP-`documentSymbol` als Verfeinerung; Outline als Dock-Pane der IDE; Daten-Formate (JSON/TOML/YAML).

## #48 Git-Panel gebaut (2026-10-01, erster Wurf: Changes)

Entscheidungen des Owners (2026-10-01): neues schlankes Crate `axiomata-git` (nicht in `axiomata-files`; das Crate ist
macOS-gebunden und trägt Watcher/LSP mit — mac-only Code soll später ohnehin aufgebrochen werden, damit die App auch
unter Linux/Windows läuft), erst umziehen, dann bauen, zwei Gruppen mit Hunk-Staging.

- **CP1 Umzug:** `crates/axiomata-git` (`run` = der eine Ort, der `git` aufruft, mit `GIT_OPTIONAL_LOCKS=0`,
  `GIT_LITERAL_PATHSPECS=1`, `GIT_TERMINAL_PROMPT=0`; `diff` = Typen, Parser, Hunk-Patch, `checked_path`, `ChangeKind`).
  `axiomata-ide` nutzt es über dünne Hüllen (eigener Fehlertyp bleibt), seine Tests blieben unverändert grün.
- **CP2 Engine** (`axiomata-git::repo`): `status` (porcelain v2 -z, Branch/Upstream/ahead-behind), `stage`/`unstage`
  (Datei, alles; vor dem ersten Commit geht `unstage` über `rm --cached`), `file_diff` je Seite (`Staged` = Index gegen
  `HEAD`, `Unstaged` = Arbeitsbaum gegen Index, Untracked gegen nichts), `apply_hunk` (Stage/Unstage eines Blocks per
  `git apply --cached`, nur wenn Index+Header noch stimmen; ein ganz neuer/gelöschter Block = die Datei), `blob`
  (Datei wie `HEAD`/Index sie hat), `commit` (nur Gestagtes; verweigert leere Nachricht/leeren Index), `fetch`
  (liest nur) und `push` (Owner-Wunsch 2026-10-01: Push und Commit & Push; nur der ausgecheckte Branch, zu seinem
  Upstream bzw. als neuer Branch zu `origin`, **nie mit `--force`**, Remote und Ref nennt nie die Oberfläche — eine
  abgelehnte Übertragung ändert nichts und zeigt Gits Meldung; scheitert der Push nach „Commit & Push“, bleibt der Commit
  und die Meldung sagt es). Gegen echte Repositories (auch ein lokales Remote) getestet.
- **CP3 Tauri:** `src-tauri/src/git.rs` — `git_status|stage|unstage|stage_all|unstage_all|diff|blob|apply_hunk|commit|fetch`
  auf `project:<id>`-Wurzeln (`files::project_folder` lässt nur Projekt-Wurzeln zu).
- **CP4 Oberfläche:** dritter Reiter „Git“ in der linken Spalte (`GitPanel.svelte`: Branch, ↑↓, Fetch, Push (*Push ↑N* / *Publish branch*); Gruppen Staged/
  Changes; Stage/Unstage je Datei und alle; Commit-Feld, ⌘⏎, *Commit* und *Commit & Push*), ein Klick auf eine Datei öffnet die Änderung als
  `GitDiffView.svelte` über dem Editor (derselbe `DiffPanes` wie der Diff der IDE; Knopf „Stage“/„Unstage“ je Hunk, „Stage
  file“, Layout-Umschalter; liest sich alle 5 s neu, ohne Falten/Cursor zu verlieren).
- **Abrundung (2026-10-01, Owner: „Git-Tasks fertig machen“):** `init` (Knopf „Create repository“; verweigert in/unter einem
  Repository), `discard` (Datei: Unstaged zurück auf den Index, Untracked wird gelöscht — nur nach Rückfrage in der Oberfläche;
  Gestagtes bleibt) und `discard_hunk` (je Block im Diff, neben „Stage“), Branches (`branches`, `switch_branch`,
  `create_branch`; Wechsel verweigert, wenn Änderungen im Weg sind — kein stilles Stash; Namen prüft `git check-ref-format`),
  Branch-Menü im Panel-Kopf, und das Panel als **Git-Pane der IDE** (`ide/panes/GitPane.svelte`, `+`-Menü → Git; der Diff
  öffnet sich über dem Pane, „Datei öffnen“ in einem Datei-Pane daneben).
- **Offen:** History (Log + Diff je Commit), Remote-Branches wechseln, Stash, Merge/Rebase/Konflikte lösen, Tags. Die Tauri-Hülle
  ist auf der Linux-Box nicht kompiliert.

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

## #50 Run/Tasks — erster Wurf gebaut (2026-10-02)

Pure Logik in der neuen Crate **`axiomata-tasks`** (kein Tauri, keine DB; baut und testet überall): Erkennung aus `Cargo.toml`
(`cargo build/check/clippy/test/run`), `package.json` (ein Task je Script, Paketmanager nach Lockfile, ohne `pre*`/`post*`-Hooks)
und `pyproject.toml`/`pytest.ini` (`pytest`, `ruff check`, mit `uv run` bei `uv.lock`); `tasks.json` im Projekt
(`.axiomata/tasks.json`) und persönlich (`~/.axiomata/tasks.json`), streng gelesen (nur Ordner im Projekt, gültige
Umgebungsnamen, Obergrenzen, Doppelte und Fehler als `problems` sichtbar). **Trust:** Detected und Personal laufen ohne Frage; die
Projektdatei erst nach Bestätigung, gespeichert als SHA-256 ihrer Bytes (`~/.axiomata/task-trust.json`, 0600, atomar) — jede Änderung
fragt neu. Die Bestätigung nimmt nur den Hash an, der noch zur Datei auf der Platte passt (`tasks_trust`), die Webview kann keinen
eigenen Hash eintragen. Glue: `src-tauri/src/tasks.rs` (`tasks_list`, `tasks_trust`, `task_command_line`; **nicht auf Linux
kompiliert**). Oberfläche: Rail-Icon *Run* → `ide/TasksPanel.svelte` (nach Zweck gruppiert, ▶ je Task, gesperrt mit „Review…“ bis zur
Bestätigung), Ausführung als **Task-Pane** (`ide/panes/TaskPane.svelte`): Terminal-PTY mit der Zeile als `initialCommand`; der Tab
nennt nur die Task-Id, die Zeile liegt nur im Speicher (`ide/taskRuns.ts`), gespeicherte Task-Panes werden beim Laden entfernt.
**Offen (nächste Checkpoints):** klickbare `Datei:Zeile`-Fehler im Ausgabe-Pane, Problem-Matcher + Diagnosen im Editor (offen:
wann verschwinden sie?), Abbrechen ohne Neustart, Umgebung wie `toolenv.rs`.

**Eigene Tasks im Panel (2026-10-02, Owner-Wunsch):** „New task…" im Run-Panel — Name, Befehl, Ordner (optional), Zweck, und wo es
liegt: *in this project* (`.axiomata/tasks.json`, reist mit dem Repository) oder *for all my projects* (`~/.axiomata/tasks.json`).
Eigene Tasks lassen sich bearbeiten und entfernen. Rust: `upsert_task`/`remove_task` arbeiten auf dem JSON selbst (Fremdes bleibt
stehen, eine ungültige Datei wird nie überschrieben, doppelte Namen und Pfade nach draußen werden abgewiesen), `write_project_file`
schreibt atomar und **nie durch einen Symlink** (`.axiomata` oder die Datei könnten in einem geklonten Repo einer sein).
**Vertrauen:** Speichert der Owner in eine Projektdatei, die er bereits bestätigt hatte (oder die es noch nicht gab), gilt der neue
Inhalt als bestätigt; war sie **nicht** bestätigt (kann Fremdes enthalten), bleibt sie es — sonst würde das Speichern still
fremde Befehle freigeben. Glue: `tasks_save`, `tasks_remove` in `src-tauri/src/tasks.rs` (nicht auf Linux kompiliert). Umgebungsvariablen
sind im Formular noch nicht einstellbar (nur in der Datei).

**#50 Nachzug (2026-10-02):** ⌘-Klick auf `Datei:Zeile` in der Ausgabe öffnet die Stelle im Editor — in Task-Panes (Pfad nur innerhalb des
Projekts, `ide/outputPath.ts`) und in Agent-Panes (innerhalb des Agent-Worktrees). Erkannt werden rustc/gcc/pytest/ruff/eslint-Form
`pfad:zeile:spalte`, Python-Tracebacks (`File "…", line N`) und tsc (`pfad(zeile,spalte)`); URLs und `host:port` nie
(`core/outputLinks.ts`). Der Terminal-Baustein bekommt dafür nur den optionalen Prop `onLink`; der Text kommt aus der Shell und
bestimmt nie selbst, wohin geöffnet wird. Neu außerdem: **Stop** (Ctrl-C an die Shell, sie bleibt) im Task-Pane.
**Weiterhin offen:** Problem-Matcher mit Diagnosen im Editor (Frage: wann verschwinden sie? Vorschlag: beim nächsten Lauf derselben Task
und wenn ihr Pane geschlossen wird) und die Umgebung wie `toolenv.rs`.

## Debug (#51) — Stand 2026-10-02: Python zuerst (Owner: „denke die weiteren Sprachen sind dann eh einfacher")

Gebaut: Crate `axiomata-dap` (Content-Length-Framing, `Client`, `Session` mit Start-Reihenfolge initialize → launch →
`initialized` → Breakpoints → `configurationDone`; Reverse-Requests werden abgelehnt), `debug.json`
(`.axiomata/debug.json`, Hash-Bestätigung wie bei Run) plus erkannte Konfigurationen (pytest, `__main__.py`,
`main.py`/`app.py`/`manage.py`) und „Current file". Adapter: `.venv`-Python mit debugpy, sonst
`uv run --with debugpy`, sonst System-Python, sonst eine Meldung. Tauri-Glue `src-tauri/src/debug.rs`
(auf dem Linux-Rechner nicht kompilierbar — Mac-Test steht aus). Frontend: Breakpoints per Klick auf die
Zeilennummer (rote Pille, `settings.ide.breakpoints`, folgen Umbenennungen), Debug-Ansicht in der Rail
(Toolbar F5/F10/F11, Call Stack, Variablen, Breakpoint-Liste, Konsole mit Evaluate), Datei öffnet sich am Stopp.

Eigene Konfigurationen werden im Panel angelegt (Formular „New configuration…“, schreibt `.axiomata/debug.json`, ohne dass man eine Datei anfassen muss).

Offen: Terminal-Pane für TUI-Programme (`runInTerminal`); Breakpoints wandern nicht mit, wenn Zeilen davor eingefügt werden; Rust (`lldb-dap`) und Node als
weitere Adapter; Watch-Ausdrücke, bedingte Breakpoints.

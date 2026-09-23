# Detailplan: M7.3 — Git-Schicht (CP7–CP9)

Status: **gegrillt und bestätigt; CP7 gebaut und geprüft**, CP8/CP9 offen (Stand 2026-09-23).
Gehört zu [`agentic-ide.md`](agentic-ide.md) §5, M7.3. Vorgänger: M7.2 CP5 (ein
Worktree pro Agent, `git` als Unterprozess — F3) und CP6 (Status-Kanal, der sagt,
wann ein Agent fertig ist).

## Kontext

Jeder Agent arbeitet seit CP5 in seinem eigenen Worktree auf seinem eigenen Branch
(`axiomata/<slug>-<id>`). Was er dort tut, sieht man heute nur im Terminal oder in
einer eigenen Shell mit `git diff`. M7.3 macht daraus den eigentlichen Nutzen von E4
(„nur so sind die Diffs pro Agent trennbar"):

- **CP7** — eine Git-Engine in `axiomata-ide`, die sagt, *was der Agent verändert
  hat*, und die Handgriffe darauf ausführt (stagen, verwerfen, committen, mergen).
- **CP8** — der Diffs-Tab am Fensterrand jedes Agenten: geänderte Dateien, Diff-Ansicht.
- **CP9** — aus dem Diff heraus handeln: Datei öffnen und bearbeiten, pro Datei
  stagen/verwerfen, committen, und den Agenten-Branch in den Hauptbaum übernehmen.

Die Engine kommt zuerst und ist ohne Oberfläche testbar — dieselbe Reihenfolge wie
beim Terminal und beim Board (§8 des Dachplans).

## Was schon da ist

- `worktree.rs`: `git()`-Helfer (Unterprozess, stderr in `IdeError::Git`),
  `is_repo`, `add`/`remove`/`list`, `branch_exists`, `has_uncommitted_changes`.
- `provision.rs`: kennt Projekt ↔ Agent ↔ Worktree ↔ Branch.
- Frontend: der Diffs-Tab existiert als deaktivierter Eintrag in `AgentPane.svelte`;
  der Status-Takt (`ide/agentStatus.ts`) weiß, wann ein Agent von `working` auf
  `idle` fällt; der FileViewer (`md-file`, schwebendes Panel) liest und editiert alle
  Textformate.

## Entwurf

### CP7 — Git-Engine (`crates/axiomata-ide/src/git.rs`, neu)

Weiter über den `git`-Unterprozess (F3), ohne neue Abhängigkeit. Alles, was git
liefert, wird mit `-z` bzw. `--porcelain=v2` gelesen, nie aus menschenlesbarer
Ausgabe geraten.

- **Die Basis eines Agenten** (siehe G1): der Stand, von dem aus „was hat der Agent
  verändert" gemessen wird.
- `changes(worktree, base) -> Vec<FileChange>` — pro Datei `path`, `old_path`
  (Umbenennung), `kind` (added/modified/deleted/renamed/untracked), `staged`,
  `additions`/`deletions`, `binary`, `uncommitted`. Committete Arbeit des Agenten
  **und** der uncommittete Stand im Worktree zusammen (G2).
- `file_diff(worktree, base, path) -> FileDiff` — strukturiert: Hunks mit Zeilen
  (`context`/`add`/`remove`, alte und neue Zeilennummer). **In Rust geparst**, damit
  die Oberfläche nichts parsen muss und der Parser Tests hat. Obergrenze für Größe
  und Zeilen; Binärdateien nur als „binär, n Bytes".
- Handgriffe im Worktree: `discard(paths)` auf den Stand der Basis (G13),
  `commit_all(message)` (G3). Kein Stagen einzelner Dateien — mit „der Agent
  committet selbst" und „Commit-Knopf für alles Offene" gibt es dafür keinen Anlass.
- `take_over(...)` — G7–G12; der einzige Handgriff, der den **eigenen Arbeitsbaum
  des Nutzers** anfasst.
- Befehlsschicht: Tauri-Befehle plus `axiomata-cli ide agents diff|commit|discard|
  take-over`, damit alles ohne Oberfläche prüfbar ist.
- Pfade werden gegen die Worktree-Wurzel geprüft (kein `..`, kein absoluter Pfad) —
  derselbe Fehlerfall wie `prepend_files` (§7 des Dachplans).

### CP8 — Diffs-Tab (`AgentPane.svelte` + `ide/git.ts`, `ide/DiffView.svelte`)

- Links die Dateiliste (Symbol für die Art, `+n −m`), rechts der Diff der gewählten
  Datei; Zeilennummern, Hinzugefügtes/Entferntes über `--ax-*`-Token.
- Aktualisierung nach G4; Übernehmen-/Commit-Knöpfe kommen erst mit CP9.
- Kein Worktree (geteilter Ordner): der Tab sagt, warum es keinen Diff pro Agent
  gibt, statt leer zu sein.

### CP9 — Handeln aus dem Diff

- Datei im FileViewer öffnen — aus dem **Worktree des Agenten**, Hinweis während
  `working` (G6).
- Pro Datei verwerfen (G13, mit Rückfrage), Commit-Knopf (G3).
- „In den Hauptbaum übernehmen" (G7–G12), dazu der Diff als Dock-Pane (G5).

## Entscheidungen (gegrillt 2026-09-23, Runden Q1–Q13, alle vom Owner bestätigt)

- **G1 — Basis = gespeicherter Ausgangsbranch** (Q1). Beim ersten Anlegen des
  Worktrees wird der im Projektordner ausgecheckte Branch als `base_branch` am
  Agenten gespeichert (Migration, `SCHEMA_SQL_V4`). Agenten von vorher: Rückfall auf
  den gerade ausgecheckten Branch. Verglichen wird gegen die **Merge-Base** von Basis
  und Agenten-Branch — nicht gegen die Spitze der Basis, sonst sähe man fremde
  Änderungen an `main` als Rücknahmen des Agenten.
- **G2 — Ein Diff „alles seit der Basis"** (Q2): Committetes und der offene Stand im
  Worktree zusammen, pro Datei markiert, ob noch uncommittete Änderungen dabei sind.
- **G3 — Der Agent committet selbst** (Q3); die Oberfläche hat einen Commit-Knopf für
  Liegengebliebenes (committet alles Offene im Worktree).
- **G4 — Aktualisierung** (Q4): beim Öffnen des Tabs, beim Wechsel `working → idle`,
  per ↻, und alle 5 s solange der Tab sichtbar ist **und** der Agent `working` ist.
- **G5 — Ort** (Q5): CP8 Seiten-Tab „Diffs"; CP9 zusätzlich als Dock-Pane.
- **G6 — Bearbeiten während `working`** (Q6): erlaubt, mit deutlichem Hinweis.
- **G7 — Übernehmen per Squash**, `--no-ff` wählbar (Q7). Nachricht vom Nutzer.
- **G8 — Schmutziger Hauptbaum** (Q8): erlaubt, solange git selbst nicht wegen
  Kollision verweigert. **Zusätzlich verweigert, wenn im Hauptbaum etwas gestaged
  ist** — ein Squash-Commit nähme sonst die gestagten Änderungen des Nutzers mit.
- **G9 — Konflikt ⇒ sofort zurück** (Q9): Squash per `git reset --merge`, `--no-ff`
  per `git merge --abort`; die Konfliktdateien werden genannt.
- **G10 — Nur in die ausgecheckte Basis** (Q10): ist im Projektordner ein anderer
  Branch aus, wird mit einem klaren Satz verweigert; nie selbst umschalten.
- **G11 — Nach dem Übernehmen** wird der Agenten-Branch auf die neue Basis gesetzt
  (Q11): nach Squash `reset --hard <basis>` (der Worktree ist dann nachweislich
  sauber, G12), nach `--no-ff` ein Fast-Forward. Der Diff ist danach leer.
- **G12 — Nur committeter Stand, nie während `working`** (Q12): offene Änderungen ⇒
  Fehler „erst committen"; der Status-Kanal (CP6) sperrt das Übernehmen, solange der
  Agent `working` ist.
- **G13 — Verwerfen = auf den Stand der Basis** (Q13): auch schon Committetes; neu
  angelegte Dateien werden gelöscht. Immer mit Rückfrage; das Ergebnis ist eine
  uncommittete Änderung, also sichtbar und rückgängig zu machen.
- **Außerdem festgehalten:** M7.3 **pusht nie**. Commits laufen mit der Git-Identität
  und den Hooks des Nutzers (Unterprozess, F3). Jeder Pfad aus der Oberfläche wird
  relativ zur Worktree-Wurzel geprüft (kein `..`, nicht absolut).

## CP7 — was die Prüfungen geändert haben

Vier Sub-Agenten (Sicherheit, Architektur, Tests, Performance) liefen über CP7;
eingearbeitet wurde:

- **Pfade sind wörtlich** (`GIT_LITERAL_PATHSPECS=1` in jedem git-Aufruf). Ohne das
  machte eine Datei namens `:(glob)*.txt` aus „diese Datei verwerfen" ein „alle
  passenden verwerfen" — ein `--` davor hilft nicht (experimentell bestätigt).
- **Ein vom Hook abgelehnter `--no-ff`-Merge wird zurückgenommen.** Ohne
  Konfliktmarker, aber mit `MERGE_HEAD` und gestagtem Ergebnis hätte sonst der
  nächste, unabhängige `git commit` des Nutzers den abgelehnten Merge vollendet.
- **Sonderdateien werden nie geöffnet** (FIFO, Socket, Gerät): ein `mkfifo` im
  Worktree hätte sonst bei jeder Abfrage einen Thread für immer blockiert.
- **Zu große Dateien werden gar nicht erst gedifft** (Größe vorab, nicht erst nach
  dem Einlesen der ganzen Ausgabe).
- **„Liegt in der Basis?" über `ls-tree`, nicht `cat-file -e`** — Letzteres endet bei
  „gibt es nicht" mit 128, demselben Code wie jeder fatale Fehler; ein echter
  Fehler hätte zur Löschung geführt.
- **Verwerfen ist umbenennungsbewusst** (der neue Name bringt den alten zurück) und
  überspringt nie still.
- **Die Sperre „nicht während `working`" hat genau einen Ort:**
  `provision::TakeOverTarget::run`; `AgentRepo::take_over` ist crate-privat, kein
  Aufrufer kann die Prüfung vergessen.
- **Drei Zustände statt `Option`:** `AgentRepoState::{Ready, SharedFolder,
  NotStarted}` — der Diffs-Tab sagt für „noch nie gestartet" etwas anderes als für
  „geteilter Ordner".
- **Zwei git-Aufrufe pro Abfrage statt fünf** (`status` liefert auch die
  unversionierten Dateien; ein `diff --raw --numstat`), und die zwei alten
  CP5-Befehle halten die DB-Verbindung nicht mehr während git läuft.

## Verifikation

- Engine-Tests gegen echte temporäre Repositories (wie `provision.rs`): Änderungen
  aller Arten, Umbenennung, Binärdatei, leerer Diff, Pfad mit Leerzeichen und
  Umlaut, Parser gegen echte `git diff`-Ausgabe, Obergrenzen, Pfad-Ausbruch
  verweigert, Merge sauber / mit Konflikt / bei schmutzigem Hauptbaum.
- `cargo build/test/clippy/fmt`, `npm run check`, `npx vitest run`.
- Browser-Weg über `devmock.ts`-Fixtures für Dateiliste und Diff.
- Live am echten Mac mit CC und OC, **ohne** Oberflächen-Änderungen während des Tests.
- Sub-Agenten pro Checkpoint: `rust-test-engineer`, `security-auditor` (schreibende
  Git-Handgriffe, Pfade aus der Oberfläche, Merge in den Arbeitsbaum des Nutzers),
  `architecture-reviewer`, `docs-writer`, `rust-performance-analyzer` (große Diffs).

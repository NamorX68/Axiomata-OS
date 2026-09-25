# Detailplan: M7.3 — Git-Schicht (CP7–CP9)

Status: **M7.3 fertig** — CP7, CP8 und CP9 gebaut und geprüft (CP8/CP9 auf dem neuen Editor, H1–H16,
2026-09-24).
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

## CP8/CP9 auf dem Editor (gegrillt 2026-09-24, Runden Q1–Q16, alle vom Owner bestätigt)

Seit dem Editor-Plan (`editor.md`, D15) ist die Diff-Ansicht ein schreibgeschützter Editor
mit Markierungen, und „Datei öffnen" öffnet den Editor statt des FileViewers. Die
Entscheidungen heißen **H**, weil G schon vergeben ist (hier und im Editor-Plan).

- **H1 — Einspaltig und nebeneinander** (Q1), umschaltbar per Knopf und ⌘⇧D, die Wahl wird
  gemerkt. Vorgabe im schmalen Seiten-Tab einspaltig, in der Dock-Pane nebeneinander.
- **H2 — Volltexte beider Seiten** (Q2): die Agenten-Seite über `file_read` auf
  `worktree:<id>`, die Basis-Seite über den neuen Befehl `ide_agent_base_text`
  (`git show <merge-base>:<pfad>`, gleiche Grenzen). Jede Seite wird für sich geparst;
  jede Diff-Zeile nimmt die Farben ihrer Seite. **Was** sich geändert hat, bleibt
  git's Hunks (passt zu `+n −m`).
- **H3 — Auf dem Editor, nicht daneben** (Q3): `EditorSurface` bekommt allgemeine
  Anschlüsse — Zeilenmarkierungen, einen austauschbaren Zeilenrand, Trennzeilen ohne
  Text. Das Diff-Modell ist reine Logik in `editor/diff/` (vitest). Derselbe Anschluss
  trägt später den Git-Gutter.
- **H4 — Wortgenaue Markierung** (Q4) für Paare aus entfernter und hinzugefügter Zeile,
  mit Obergrenze; Token `--ax-diff-add`, `--ax-diff-remove`, `--ax-diff-add-word`,
  `--ax-diff-remove-word`, `--ax-diff-hunk` in allen Themes.
- **H5 — „Datei öffnen" = IDE-Dock-Pane „Datei"** (Q5) auf `worktree:<id>`, an der
  angeklickten Zeile, mit denselben Balken wie die Datei-App und dem Hinweis aus G6.
- **H6 — Verwerfen pro Datei und pro Hunk** (Q6). Der Hunk wird per umgekehrtem Patch
  (`git apply -R`, nur dieser Hunk) zurückgesetzt, als uncommittete Änderung wie G13,
  mit Rückfrage; passt er nicht mehr, wird abgelehnt.
- **H7 — F10 „Unterschied ansehen" wird ein echter Diff** (Q7): ein Zeilen-Diff (Myers)
  in der Engine, derselbe Anzeiger; ersetzt die Platten-Fassung daneben.
- **H8 — Binär und Bilder** (Q8): Hinweis „Binärdatei geändert" mit beiden Größen;
  Bilder als Vorher/Nachher.
- **H9 — Tasten** (Q9): ⌥↓/⌥↑ Hunk, ⌥⌘↓/⌥⌘↑ Datei, ⏎ an der Zeile öffnen, ⌘⇧D
  Darstellung, ⌘⌫ Hunk verwerfen (Rückfrage), ⌘R aktualisieren; Vi (`]c`/`[c`) mit ED3.
- **H10 — Vorbelegte Nachrichten** (Q10): Übernehmen mit dem Plan-Titel (CP6b), sonst dem
  Betreff des letzten Agenten-Commits; Commit mit „wip: <Agentname>". Editierbar.
- **H11 — Kontext aufklappen** (Q11): Trennzeile „⋯ n unveränderte Zeilen" mit „↑ 20",
  „↓ 20" und „alle", dazu ein Schalter „ganze Datei".
- **H12 — Nebeneinander = zwei schreibgeschützte Editoren** auf einem ausgerichteten
  Zeilenplan (Platzhalterzeilen), synchron gescrollt über **eine** gemeinsame
  Echo-Sperre, die auch die zwei Sperren aus ED2 ersetzt (Q12).
- **H13 — Verwerfen während `working`** (Q13): erlaubt; die Rückfrage sagt deutlich, dass
  der Agent gerade arbeitet. Ein nicht mehr passender Hunk ⇒ Ablehnung, Neuladen, Hinweis.
- **H14 — Zwei neue Dock-Panes** (Q14): `agent-diff` (`config: { agentId }`, höchstens eine
  pro Agent, eigene Dateiauswahl) und `file` (`config: { root, rel }`, allgemein, auch für
  die spätere IDE-Dateiansicht; fehlende Wurzel ⇒ Hinweis statt Verschwinden; dieselbe
  Datei zweimal ⇒ die bestehende nach vorn). Beide im Projekt-Layout gespeichert.
- **H15 — Kein „Leerzeichen ignorieren"** (Q15) — die Hunks müssen zum echten Inhalt
  passen, sonst verwirft H6 etwas anderes, als man sieht. Später-Punkt.
- **H16 — Checkpoints** (Q16): **CP8a** Grundlagen (Diff-Logik, Surface-Anschlüsse,
  `ide_agent_base_text`, TS-Typen, devmock, Token) · **CP8b** Diffs-Tab (inkl. H7 und
  der gemeinsamen Echo-Sperre) · **CP9a** Dock-Panes · **CP9b** Handeln (Verwerfen pro
  Datei/Hunk, Commit, Übernehmen, Sperren). Commit nach CP8 und nach CP9.

## CP8 — gebaut (2026-09-24) und was die Prüfungen geändert haben

Gebaut wie H1–H16 es sagen: `editor/diff/` (Myers, Hunks, Wort-Diff, Modell mit Faltungen,
Pane-Dokumente, Färbung je Seite), die Dekorations-Anschlüsse an `EditorSurface`,
`fileapp/DiffPanes.svelte` (einspaltig oder zwei Editoren nebeneinander), `ide/DiffView.svelte`
im Seiten-Tab, `ide_agent_base_file` + CLI `ide agents base`, „Compare" der Datei-App als echter
Diff, eine gemeinsame Echo-Sperre (`fileapp/scrollLink.ts`). Im Browser gegen devmock geprüft
(Agent 1 hat dort einen Worktree mit allen Änderungsarten, Agent 2 den geteilten Ordner).

- **Beim Testen gefunden:** Ein 5-s-Abruf, der vor einem Dateiwechsel begann, lud danach die
  alte Datei — jede Ladung prüft jetzt, ob ihre Datei noch die gewählte ist. Beim Dateiwechsel
  fragte die Anzeige kurz die Highlighter der *alten* Datei nach Zeilen jenseits ihres Endes,
  tree-sitter warf und die Anzeige blieb stehen — die Highlighter tragen jetzt die Texte, für
  die sie gebaut sind, und `SyntaxHighlighter.spans` antwortet auf einen Bereich hinter dem
  Ende leer.
- **Tests:** Nebeneinander paarte eine geänderte letzte Zeile nicht über git's
  „No newline"-Hinweis hinweg (der Wort-Diff schon) — jetzt einheitlich.
- **Performance (HIGH):** Jeder Abruf las jede unversionierte Datei ganz, um ihre Zeilen zu
  zählen — die Zählung wird jetzt nach Größe und Änderungszeit gemerkt. Bilder beider Seiten
  laden parallel; ob sich der offene Diff geändert hat, wird Feld für Feld verglichen statt
  über `JSON.stringify`.
- **Architektur (HIGH):** „Ist das Text?" hatte zwei Regeln (Worktree: UTF-8; Basis: UTF-8
  ohne NUL) — jetzt eine, `axiomata_files::text_from_bytes`, für beide Seiten.
- **Sicherheit:** nur LOW (Symlinks zeigen ihr Ziel als Text, wie `git diff` selbst).
- **Für CP9 vorgemerkt:** Verwerfen mehrerer Dateien bündeln (ein `ls-tree`, ein `restore`
  statt zwei Aufrufen pro Pfad); die Lade-Logik von `DiffView` in eine testbare Klasse ziehen,
  wenn die Handgriffe dazukommen; die Dock-Pane braucht eine eigene Kopfzeile (welcher Agent).
- **Später:** „Leerzeichen ignorieren" (H15); bessere Paarung geänderter Zeilen nach Ähnlichkeit
  statt nach Position (heute bleibt eine eingeschobene Kommentarzeile ohne Wortmarken).

## CP9 — gebaut (2026-09-24)

- **CP9a — Dock-Panes:** `file` und `agent-diff` (`ide/paneKinds.ts`, `openOrFocus`); die
  Bearbeitung ist `fileapp/FileEditor.svelte`, aus der Datei-App herausgelöst, damit Vollbild
  und Pane gleich bearbeiten; ⏎ oder „Open" im Diff öffnet die Datei des Agenten an der Zeile,
  ein zweites Öffnen holt die Pane nach vorn und springt; „⧉ Dock" im Seiten-Tab; der Hinweis
  aus G6, solange der Agent arbeitet; schmale Panes stapeln Liste über Diff.
- **Beim Testen gefunden:** Der Dock verschiebt Panes bei jeder Layout-Änderung im DOM, was
  Scrollpositionen still auf 0 setzt — eine Editor-Oberfläche zeichnete dann die falschen
  Zeilen über einen leeren Streifen. Elemente mit `data-keep-scroll` bekommen ihre Position
  zurück (`ide/paneStore.ts`). `goToLine` scrollt erst nach dem nächsten Render (vorher war
  die Fläche einer frisch geöffneten Datei noch nicht hoch genug).
- **CP9b — Handeln:** Verwerfen pro Datei (gebündelt: ein `ls-tree`, ein `restore`, ein
  `rm --cached`) und pro Hunk (`discard_hunk`: der Hunk wird als Patch mit C-gequoteten Namen
  gebaut und über stdin rückwärts angewandt; ein Hunk, der nicht mehr so aussieht wie gezeigt,
  wird verweigert, H13); Kopfzeile mit „Discard" über jedem Hunk, ⌘⌫ im Hunk; Commit- und
  Übernehmen-Dialog (`ide/GitActionDialog.svelte`) mit Vorbelegung (H10: Überschrift des
  Plan-Dokuments, sonst `ide_agent_last_subject`; die Aufgabenliste hat keinen Titel);
  „Take over" gesperrt mit Grund, solange der Agent arbeitet oder Uncommittetes da ist (G12).
  Die Lade- und Handgriff-Logik steckt in `ide/diffSession.svelte.ts` (Architektur-Befund CP8).
  `parse_diff` behält jetzt ein `\r` am Zeilenende, sonst passte ein Hunk-Patch nicht auf
  CRLF-Dateien.

### CP9 — was die Prüfungen geändert haben

- **Sicherheit (LOW):** Der Patch für `git apply` wird jetzt auf einem eigenen Thread in stdin
  geschrieben, während stdout/stderr gelesen werden — ein Hunk kann bis 2 MiB groß sein und
  jeden Pipe-Puffer übersteigen. Experimentell bestätigt: `git apply` ohne `--unsafe-paths`
  weist `..`, absolute Pfade und Pfade hinter Symlinks selbst ab.
- **Architektur (MEDIUM):** `data-keep-scroll` war ein Textvertrag über die Schichtgrenze; jetzt
  eine Konstante in `fileapp/keepScroll.ts` (die Datei-App darf die IDE nicht importieren),
  gesetzt per `{...KEEP_SCROLL}`. Dazu: nur Dateien sammeln sich in einer Gruppe, eine
  Diff-Pane dockt neben ihrem Agenten; ein gemeinsames `core/errors.ts` `messageOf`; der
  G6-Hinweis liest den Agentennamen reaktiv.
- **Vorgemerkt:** Seiten-Tab und Dock-Pane desselben Agenten haben je eine eigene Sitzung und
  fragen beide alle 5 s ab, wenn beide sichtbar sind — bei Bedarf eine Sitzung pro Agent teilen.
  `base_paths` unterscheidet nicht nach Eintragsart: Ein Pfad, der in der Basis ein Ordner ist,
  wird beim Verwerfen wiederhergestellt statt gelöscht (so gewollt, wie vor CP9).

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

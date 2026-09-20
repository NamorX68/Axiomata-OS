# Plan: Kanban-Brett (M7.0)

Status (2026-09-20): **KOMPLETT.** Alle Checkpoints umgesetzt, live geprüft und
committet (`e70ca08` bis `6096acd`). Erster Meilenstein der M7-Kette
(`docs/plans/agentic-ide.md`), bewusst als **eigenständiges Vorhaben**
geschnitten: das Brett ist ohne einen einzigen Agenten nützlich und soll die App
später auch verlassen können.

Was steht: die Crate `axiomata-board` (Domänentypen, alle SQL-Anweisungen, das
initiale Schema als `SCHEMA_SQL_V1`, ohne Abhängigkeit auf `axiomata-core`),
Migration 0008 samt WAL-Umstellung der ganzen Datenbank, `axiomata-cli board
…` mit neun Verben, das Dashboard-Modul in drei Betriebsarten (Kachel, großes
Panel, Kartendetail), Ziehen **und** Tastaturumzug über dieselben reinen
Funktionen, Spaltenverwaltung am Brett, drei Modul-Actions für den Chat, und der
einseitige Markdown-Spiegel im Vault. 50 Tests in der Crate, 31 in
`core/kanban.ts`, 7 in `core/boardStore.ts`.

**Gegenüber diesem Plan geändert** (jeweils weil das Bauen es gezeigt hat):
Ein zusätzlicher Checkpoint **CP-K2-Design** kam vor CP-K2a — die Kartenform
wurde an einer Wegwerfansicht entschieden statt beschrieben, und dabei kamen die
`--ax-card-*`- und `--ax-label-*`-Tokens heraus. CP-K2 wurde in ein lesendes
(K2a) und ein schreibendes (K2b) geteilt, damit die Gestaltung landet, bevor
Drag-and-Drop das DOM verkompliziert.

## 1. Warum zuerst

Drei Gründe, in dieser Reihenfolge:

1. **Für sich allein nützlich.** Ein Kanban für die eigene Arbeit, unabhängig
   von allem, was in M7.1 und später kommt.
2. **Es ist die Datenschicht des Agenten-Bretts.** In M7.5 beanspruchen Agenten
   Karten, melden sie fertig und lassen sie gegenseitig abnehmen. Wird das Brett
   danach gebaut, wird es zweimal entworfen.
3. **Es ist der kleinste Meilenstein mit echtem Ergebnis** — ein guter Auftakt
   für eine lange Kette.

## 2. Entscheidungen (Owner, 2026-09-20)

- **K-E1 — Datenbank ist die Quelle der Wahrheit, plus einseitiger
  Markdown-Spiegel in den Vault.** Transaktionen sind die Voraussetzung dafür,
  dass das atomare Beanspruchen später überhaupt funktioniert; der Spiegel sorgt
  dafür, dass Obsidian, die Second-Brain-Suche und der Memory-Router das Brett
  trotzdem sehen. In Obsidian ist es damit lesbar, nicht editierbar.
- **K-E2 — Herauslösbar wie Terminal und IDE.** Der Kern kommt in eine eigene
  Crate `axiomata-board`, die nichts von Axiomata weiß, was sie nicht wissen
  muss. Der Owner kann sich gut vorstellen, das Kanban später eigenständig
  weiterzuentwickeln.
- **K-E3 — Von Anfang an ansehnlich.** Die Strategie ist „erst von allem eine
  erste Fassung, dann ausbauen und polieren" — *aber* die erste Fassung soll
  schon gut aussehen. Maßstab ist das Niveau der zuletzt überarbeiteten Module
  (Kalender, Terminal-Einstellungen), nicht ein Funktionsgerüst zum
  Nachschärfen. Jede Farbe und Größe über `--ax-*`-Tokens, keine Literale.
- **K-E4 — Agententauglich modelliert, menschlich bedient.** v1 zeigt nur, was
  ein Mensch braucht. Das *Modell* enthält von der ersten Zeile an Zuständige,
  Beanspruchung und Status-Gates — Nebenläufigkeit lässt sich nicht nachrüsten,
  sobald Karten und Zustände ohne sie existieren.

## 3. Abgrenzung zum bestehenden `todo`-Modul

Das `todo`-Modul bleibt unangetastet: eine flache persönliche Checkliste auf
`ToDo.md` im Vault (`core/todo.ts`, `modules/todo.svelte`), offen/erledigt,
sonst nichts. Das Kanban ist die strukturierte Ebene darüber — Spalten,
Zuständige, Status-Gates, später Agenten. Kein Ersatz, keine Migration, keine
Synchronisation zwischen beiden. Ob das Kanban das ToDo irgendwann ablöst,
entscheidet sich, nachdem beide eine Weile nebeneinander liefen (K-F4).

## 4. Architektur

### Crate `crates/axiomata-board` (neu)

Enthält Domänentypen, das Schema und die Operationen — **aber weder den
Dateipfad noch die Verbindung**. Jede Funktion nimmt ein `&Connection`
entgegen. Damit gilt:

- `axiomata-core` reicht seine bestehende Verbindung herein und nimmt das
  Schema als Migration `0008` in seine `MIGRATIONS`-Liste auf (die Crate stellt
  das SQL als `pub const SCHEMA_SQL_V1` bereit — der `_V1`-Name sagt an jeder
  Aufrufstelle, dass die Konstante eingefroren ist, und ein Prüfsummentest
  bricht, falls jemand `schema.sql` doch nachträglich ändert). Eine
  Migrationskette, eine
  Verbindung, kein zweiter Datenbank-Handle auf dieselbe Datei.
- `axiomata-ide` kann später dieselbe Crate nutzen, **ohne** über
  `axiomata-core` zu gehen — genau die Abhängigkeitsrichtung, die den IDE-Schnitt
  aus dem Dachplan rettet.
- Eigenständig betrieben öffnet die Crate einfach ihre eigene Verbindung.

Abhängigkeiten: `rusqlite`, `serde`, `chrono`, `thiserror` — alle schon in
`[workspace.dependencies]`.

### Datenmodell

So gebaut (vgl. `crates/axiomata-board/src/schema.sql`):

```sql
boards        (id, name, created_at, updated_at)
board_columns (id, board_id, name, position, maps_to_status)  -- open|doing|done
cards         (id, board_id, column_id, position,
               title, body, labels,            -- labels: JSON-Array
               assignee,                       -- "human:owner" | "agent:claude-1"
               claimed_by, claimed_at,
               verified_by, verified_at,
               due_at, archived_at, created_at, updated_at)
```

Drei Abweichungen vom ersten Entwurf oben, alle bewusst:

- **Die Karte hat kein `status`-Feld.** Der Status *ist* `maps_to_status` der
  Spalte, in der sie liegt. Zwei Wahrheiten wären lautlos auseinandergelaufen.
- **Ein einheitlicher Akteur-String** statt `assignee_kind`/`assignee_id`. Die
  Zwei-Parteien-Regel vergleicht Akteure byteweise — zwei verschieden geformte
  Identitäten hätten sie bedeutungslos gemacht.
- **`archived_at`** kam dazu: Erledigtes wird archiviert, nicht gelöscht, sonst
  wächst die Fertig-Spalte unbegrenzt.

Dazu zwei Dinge, die der Entwurf noch nicht hatte und die die Prüfungen
verlangt haben: ein **zusammengesetzter Fremdschlüssel** `(column_id, board_id)`,
damit eine Karte nicht auf die Spalte eines fremden Bretts zeigen kann, und die
Zwei-Parteien-Regel zusätzlich als **`CHECK`-Constraint** — die `WHERE`-Klausel
macht daraus ein sauberes `false`, das Constraint fängt jeden künftigen
Codepfad, der am Store vorbeischreibt.

`position` ist ein `REAL`: Einfügen zwischen zwei Karten ist der Mittelwert der
Nachbarn, also ein einziges `UPDATE` statt einer Neunummerierung der ganzen
Spalte.

Zwei Regeln sind der eigentliche Kern und gehören in die Crate, nicht in die
Oberfläche:

- **Beanspruchen ist ein Compare-and-Swap:**
  `UPDATE cards SET claimed_by = ?, claimed_at = ? WHERE id = ? AND claimed_by IS NULL`
  — Erfolg heißt „genau eine Zeile geändert". Wer verliert, bekommt ein
  sauberes `false`, keine Fehlermeldung.
- **`verified` verlangt eine andere Partei:** der Übergang nach `verified` wird
  abgelehnt, wenn `verified_by == claimed_by`. Kein Agent (und kein Mensch)
  nickt die eigene Arbeit ab.

### Oberfläche

- `modules/kanban.svelte` + `modules/kanban-settings.svelte`, registriert in
  `modules/index.ts`. **Kein Singleton** — anders als das Routines-Board zeigt
  eine Kachel *ein* Brett, und zwei Bretter nebeneinander sind sinnvoll. Welches
  Brett, steht in der Instanz-Config (Flip-Rückseite).
- `core/kanban.ts` — reine Logik ohne DOM und ohne Tauri: Positionsberechnung
  beim Verschieben, Gruppierung nach Spalte, Filter, Statusübergänge. Unit-getestet
  wie `core/routineInterval.ts` und `core/todo.ts`.
- Tauri-Befehle nach dem Muster der Routinen (`commands.rs`), gebaut als
  sechzehn dünne Durchreichen: Bretter (`list_boards`, `get_board`,
  `create_board`, `rename_board`, `delete_board`, `count_board_cards`), Spalten
  (`list_board_columns`, `create_board_column`, `update_board_column`,
  `delete_board_column`) und Karten (`list_board_cards`, `create_card`,
  `update_card`, `move_card`, `delete_card`, `set_card_archived`). `claim` und
  `verify` bleiben der CLI vorbehalten — mit einem einzigen Menschen kann
  niemand abnehmen, was er selbst beansprucht hat (K-F2 zu Q1).
- **Modul-Actions** (`ModuleDefinition.actions`), damit der bestehende
  Agenten-Bridge das Brett ohne jede neue Infrastruktur bedienen kann:
  `add_card`, `move_card`, `list_cards`. Ein früher, billiger Gewinn — der
  Chat-Agent kann Karten anlegen, lange bevor es M7.5 gibt.
- `core/devmock.ts` bekommt Fixtures, damit das Modul im reinen Browser
  (`npx vite --port 1420`) benutzbar ist.

## 5. Checkpoints

### CP-K1 — Kern und Persistenz

- Crate `axiomata-board` anlegen, in die Workspace-Member aufnehmen.
- Domänentypen mit `serde`, Schema-SQL, Operationen gegen `&Connection`.
- Migration `0008_boards.sql` in `axiomata-core` einhängen.
- `axiomata-cli board list|add|move|claim|done|verify` — das Brett ist damit
  vollständig ohne Oberfläche prüfbar, so wie `routines` es vormacht.
- **Tests:** Nebenläufigkeit (zwei gleichzeitige Ansprüche auf dieselbe Karte,
  genau einer gewinnt), Gate (`verified_by == claimed_by` wird abgelehnt),
  Positionsberechnung beim Verschieben, Migration läuft auf leerer und auf
  bestehender Datenbank.

### CP-K2 — Dashboard-Modul

- `core/kanban.ts` mit Tests zuerst, dann die Komponente.
- Spaltenansicht, Karten per Drag zwischen und innerhalb von Spalten,
  Anlegen/Bearbeiten/Löschen, Filter nach Label und Zuständigem.
- Karten-Detail als schwebendes Panel über `core/staging.ts` (wie der
  Dateibetrachter), nicht auf der Flip-Rückseite — Kartentexte können lang
  werden, und die Rückseite gehört den Einstellungen (K-F2).
- Registrierung, Icon, Default-Größen, Modul-Actions, devmock-Fixtures.
- **Gestaltung ist Teil dieses Checkpoints, nicht eines späteren** (K-E3):
  Token-Treue, ruhige Spaltenköpfe, lesbare Karten bei kleiner Kachel, sinnvolles
  Verhalten auf 21:9.

### CP-K3 — Vault-Spiegel

- Einseitiges Rendern nach `<workspace>/Kanban/<brett>.md`: Spalten als
  Überschriften, Karten als GFM-Aufgabenliste mit Metadaten.
- Kopfzeile im Dokument, die unmissverständlich sagt, dass Änderungen an der
  Datei beim nächsten Schreiben überschrieben werden.
- Geschrieben mit Entprellung beim Ändern, über den bestehenden
  Workspace-Schreibpfad. Der Memory-Router findet es danach von selbst, weil er
  den Workspace ohnehin abläuft.

## 6. Entschieden (vormals offene Fragen)

- **K-F1 — Mehrere Bretter ab v1?** Ja. Modell und Oberfläche können mehrere;
  eine Kachel zeigt eines, gewählt auf der Flip-Rückseite.
- **K-F2 — Karten-Detail: Panel oder Flip-Rückseite?** Schwebendes Panel — und
  es geht **mittig auf der Kachel** auf, nicht in der Bildschirmmitte
  (optionaler `anchor` im geteilten Staging-Code).
- **K-F3 — Labels und Fälligkeitsdaten in v1?** Beides drin, Swimlanes nicht.
  Die Labelfarbe wird aus dem Labeltext gehasht, über `--ax-label-*`.
- **K-F4 — Löst das Kanban das `todo`-Modul ab?** **Nein**, die beiden bleiben
  dauerhaft nebeneinander (Owner, 2026-09-20).

## 6a. Was beim Bauen anders kam als gedacht

Der Vollständigkeit halber, weil jeder dieser Punkte Zeit gekostet hat und beim
nächsten ähnlichen Modul wieder auftauchen wird:

- **Der Flächen-Tonwertverlauf läuft in den Themen gegenläufig** — in graphite
  ist `--ax-surface-3` die hellste Fläche, in paper die dunkelste. Eine Karte
  „eine Stufe heller als ihre Unterlage" gibt es nicht themenübergreifend;
  tragfähig ist nur Spalte `--ax-bg` (Tischplatte) und Karte `--ax-card-bg`.
- **In dunklen Themen tragen weder Schatten noch Haarlinie.** Vier der fünf
  Themen sind dunkel, also ist Tonwerttrennung die Vorgabe und paper die
  Ausnahme.
- **Die globale Eingabefeld-Regel in `styles.css` schlägt eine scoped Klasse**
  (sie hat durch ihre `:not()`-Kette Spezifität 0-2-1). Ein leises,
  chromloses Feld in einem Modul muss spezifischer selektieren.
- **Die Dichte hängt an der Spalten-, nicht an der Brettbreite.** Und alles, was
  in der Spaltenreihe Platz belegt — die „+ Spalte"-Schaltfläche —, kann die
  Spalten unter die Schwelle drücken, ohne dass es jemand merkt.
- **`workspace::resolve` weist Verzeichnisse ab.** Wer einen Ordner auflösen
  will, kommt damit nicht durch.
- **Entprellung war nicht nötig.** Der Plan sah sie für den Spiegel vor; eine
  Mutation pro Nutzeraktion schreibt eine wenige Kilobyte große Datei, und ein
  Timer dafür wäre Mechanik ohne Gegenwert gewesen.

## 7. Verifikation

- `cargo build --workspace`, `cargo test --workspace`,
  `cargo clippy --workspace -- -D warnings`, `cargo fmt --check`.
- `cd apps/dashboard && npm run check && npx vitest run`.
- Der Kern ist ab CP-K1 eigenständig testbar, bevor eine Zeile Oberfläche
  existiert — dieselbe Reihenfolge wie beim Terminal.
- Gebündelte Sub-Agent-Läufe pro Checkpoint und vor jedem Commit, gemäß der
  Kadenz-Regel in `CLAUDE.md`. Für CP-K1 ist `rust-test-engineer` wegen des
  Nebenläufigkeitstests besonders relevant, für CP-K2 der Blick auf die
  Gestaltung.
- Live-Test am echten Mac für alles, was sich *anfühlen* muss: Drag zwischen
  Spalten, Karte anlegen ohne Maus, Lesbarkeit bei kleiner Kachel.

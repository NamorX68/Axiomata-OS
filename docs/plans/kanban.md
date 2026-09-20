# Plan: Kanban-Brett (M7.0)

Status: **Detailplan, wartet auf Bestätigung. Noch kein Code.**
Erster Meilenstein der M7-Kette (`docs/plans/agentic-ide.md`), aber bewusst als
**eigenständiges Vorhaben** geschnitten: das Brett ist ohne einen einzigen
Agenten nützlich und soll die App später auch verlassen können.

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
  das SQL als `pub const SCHEMA_SQL` bereit). Eine Migrationskette, eine
  Verbindung, kein zweiter Datenbank-Handle auf dieselbe Datei.
- `axiomata-ide` kann später dieselbe Crate nutzen, **ohne** über
  `axiomata-core` zu gehen — genau die Abhängigkeitsrichtung, die den IDE-Schnitt
  aus dem Dachplan rettet.
- Eigenständig betrieben öffnet die Crate einfach ihre eigene Verbindung.

Abhängigkeiten: `rusqlite`, `serde`, `chrono`, `thiserror` — alle schon in
`[workspace.dependencies]`.

### Datenmodell

```sql
boards  (id, name, created_at, updated_at)
columns (id, board_id, name, position, is_done_column)
cards   (id, board_id, column_id, position,
         title, body, labels,
         assignee_kind, assignee_id,          -- "human" | "agent" | NULL
         status,                              -- open | doing | done | verified
         claimed_by, claimed_at,
         verified_by, verified_at,
         due_at, created_at, updated_at)
```

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
- Tauri-Befehle nach dem Muster der Routinen (`commands.rs`): `list_boards`,
  `get_board`, `create_card`, `update_card`, `move_card`, `delete_card`,
  `claim_card`, `set_card_status`.
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

## 6. Offene Fragen

| # | Frage | Empfehlung | Fällig |
|---|---|---|---|
| K-F1 | Mehrere Bretter ab v1 oder erst eines? | Modell mehrere, Oberfläche eines pro Kachel mit Auswahl | CP-K1 |
| K-F2 | Karten-Detail: schwebendes Panel oder Flip-Rückseite? | Panel (Rückseite gehört den Einstellungen) | CP-K2 |
| K-F3 | Labels und Fälligkeitsdaten schon in v1? | Ja, beides; Swimlanes nein | CP-K2 |
| K-F4 | Löst das Kanban mittelfristig das `todo`-Modul ab? | Später entscheiden, nicht vorab | nach M7.0 |

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

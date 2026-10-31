# Storage und Wartung

Wie Axiomata seine eigenen Daten in `~/.axiomata` aufräumt, was wie lange bleibt und wie man „Jetzt aufräumen“ und die verwaisten Worktrees bedient.

## Was es ist

Axiomata hält **nur seine eigene Ablage** sauber: `~/.axiomata` und die Worktrees, die es selbst für Studio-Agenten
anlegt (`~/Worktrees/axiomata-agents`). Projekte, ihre Build-Ordner, der Vault, Skills, das eigene Hintergrundbild,
`theme.css` und Gluts Ordner `~/.glut` werden hier nie angefasst. Aufräumen läuft ohne Agent und kostet nie Geld.

Einmal pro Woche räumt die App beim Start von selbst auf, im Hintergrund. Was wie lange bleibt (Vorgaben, änderbar):

| Posten | bleibt |
|---|---|
| Läufe (Skill-Läufe mit ihrer Ausgabe) | 90 Tage, mindestens die letzten 200 |
| Läufe der Routines | 30 Tage |
| Chat-Schritte, Websuchen | je 90 Tage |
| Verlauf archivierter Karten | 90 Tage nach dem Archivieren (lebende Karten bleiben unberührt) |
| Sitzungskosten | 1 Jahr |
| App-Log | 14 Tage |
| Lauf-Log | rotiert ab 10 MB, die letzten 2 bleiben |
| Backups | die neuesten 2 je Datei |
| Status-Ordner der Agenten | solange ihr Agent existiert |
| Claude-Sitzungen | 30 Tage (nie der ganze Ordner — dort liegt die Anmeldung) |
| Datenbankdatei | wird verdichtet, wenn über 10 % frei sind |
| Verwaiste Worktrees | gehen mit ihrem Agenten; einer mit Änderungen wartet auf dich |
| Beendete Sitzungen ohne Arbeit | gehen nach mindestens einem Tag Ruhe, nach erneuter Prüfung |
| Backups der alten Rollen und Engines | bis du sie löschst — nie von selbst |

## Wo die Knöpfe sind

**Einstellungen → Speicher** (Zahnrad im Dock oder ⌘,):

- oben die Gesamtgröße von `~/.axiomata`, „Letztes Aufräumen“ und „Würde freigeben“;
- je Posten Größe, die Frist als Zahlenfeld („Tage“ bzw. „behalten“) und „gibt … frei“;
- eine Zeile **Glut** mit Gluts eigenem Platzbedarf (Snapshots, Lauf-Journale, Sitzungen, Tool-Ergebnisse …) — Glut
  räumt `~/.glut` nach eigenen Regeln selbst, einmal am Tag und mit „Jetzt aufräumen“; fehlt die Zeile, kann das
  installierte Glut das noch nicht melden;
- „Einmal pro Woche beim Start aufräumen“ (schaltet nur den Wochenlauf ab, nie den Knopf);
- „Regeln speichern“ und „Jetzt aufräumen“;
- „Verwaiste Worktrees, die auf dich warten“ mit einem Papierkorb je Worktree (nur sichtbar, wenn es welche gibt);
- „Beendete Sitzungen, die ein Aufräumen entfernt“ (vor dem Aufräumen) bzw. „Entfernte beendete Sitzungen“ (danach): die
  beendeten Studio-Sitzungen ohne eigene Arbeit, die das Aufräumen mit Worktree und leerem Branch wegnimmt;
- „Backups der alten Rollen und Engines“: je Sicherungsordner der Umstellung auf Rollen mit Stufen
  (`agents-backup-<Datum>`, ggf. mit `-2` …) Pfad, Größe und ein Papierkorb „Backup … löschen“ (nur sichtbar, wenn es
  welche gibt; [Modelle und Rollen](modelle-rollen.md)).

Im Terminal: `axiomata-cli maintenance status` (zeigt nur) und `axiomata-cli maintenance clean [--dry-run]`.

## Häufige Handgriffe

1. **Jetzt aufräumen:** Einstellungen → Speicher → „Jetzt aufräumen“. Die Rückfrage nennt, wie viel frei wird; Worktrees
   mit Änderungen bleiben.
2. **Eine Frist ändern:** die Zahl beim Posten ändern → „Regeln speichern“. Erst danach „Jetzt aufräumen“ — der Knopf
   räumt nach den gespeicherten Regeln und ist gesperrt, solange Änderungen offen sind.
3. **Einen verwaisten Worktree wegwerfen:** unter „Verwaiste Worktrees, die auf dich warten“ den Papierkorb →
   bestätigen. Die ungesicherte Arbeit darin ist danach endgültig weg.
4. **Eine Sicherung der alten Rollen löschen:** unter „Backups der alten Rollen und Engines“ den Papierkorb → bestätigen
   („Das Backup … löschen? Die alten Rollendateien und Engines darin sind endgültig weg.“) → „Backup gelöscht“.

## Fallen

- Ein Worktree mit ungesicherter Arbeit wird **nie** von selbst gelöscht; er wartet, bis du ihn wegwirfst. Wer ihn
  behalten will, übernimmt die Arbeit vorher im Studio (siehe [Studio](studio.md)).
- Löschen eines Agenten im Studio nimmt seinen Worktree und Branch mit; hat er Änderungen, fragt die App vorher.
- Build-Ordner von Projekten (`target/`, `node_modules`) sind Sache der Projekte — „Speicher“ zeigt sie nicht.
- Eine Studio-Sitzung mit Arbeit — Ungesichertes, ein Commit über ihre Basis, eine offene Karte oder ein Plan, für den
  sie gestartet wurde — räumt das Aufräumen nie weg; sie steht im Studio unter „Wartet auf dich: übernehmen oder
  verwerfen“. Master-Sitzungen bleiben immer.
- Eine Sicherung der alten Rollen wird nie von selbst gelöscht; nichts benutzt sie mehr, sie ist nur dein Rückweg.
- Die Datenbank wird nur verdichtet, wenn es sich lohnt; ein Lauf ohne fällige Posten tut das trotzdem, falls nötig.

## Siehe auch

[Einstellungen](einstellungen.md) · [Kosten](kosten.md) · [Glut](glut.md) · [Studio](studio.md) ·
[Modelle und Rollen](modelle-rollen.md)

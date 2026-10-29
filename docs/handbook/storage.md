# Storage und Wartung

Wie Axiomata seine eigenen Daten in `~/.axiomata` aufräumt, was wie lange bleibt und wie man „Clean up now“ und die verwaisten Worktrees bedient.

## Was es ist

Axiomata hält **nur seine eigene Ablage** sauber: `~/.axiomata` und die Worktrees, die es selbst für Studio-Agenten
anlegt (`~/Worktrees/axiomata-agents`). Projekte, ihre Build-Ordner, der Vault, Skills, das eigene Hintergrundbild,
`theme.css` und Gluts Ordner `~/.glut` werden hier nie angefasst. Aufräumen läuft ohne Agent und kostet nie Geld.

Einmal pro Woche räumt die App beim Start von selbst auf, im Hintergrund. Was wie lange bleibt (Vorgaben, änderbar):

| Posten | bleibt |
|---|---|
| Runs (Skill-Läufe mit ihrer Ausgabe) | 90 Tage, mindestens die letzten 200 |
| Routine firings | 30 Tage |
| Chat turns, Web searches | je 90 Tage |
| Archived card history | 90 Tage nach dem Archivieren (lebende Karten bleiben unberührt) |
| Session spend | 1 Jahr |
| App log | 14 Tage |
| Run log | rotiert ab 10 MB, die letzten 2 bleiben |
| Backups | die neuesten 2 je Datei |
| Agent status folders | solange ihr Agent existiert |
| Claude sessions | 30 Tage (nie der ganze Ordner — dort liegt die Anmeldung) |
| Database file | wird verdichtet, wenn über 10 % frei sind |
| Orphaned worktrees | gehen mit ihrem Agenten; einer mit Änderungen wartet auf dich |

## Wo die Knöpfe sind

**Settings → Storage** (Zahnrad im Dock oder ⌘,):

- oben die Gesamtgröße von `~/.axiomata`, „Last clean-up“ und „Would free“;
- je Posten Größe, die Frist als Zahlenfeld („days“ bzw. „kept“) und „frees …“;
- eine Zeile **Glut** mit Gluts eigenem Platzbedarf (Snapshots, Run journals, Sessions, Tool results …) — Glut räumt
  `~/.glut` nach eigenen Regeln selbst, einmal am Tag und mit „Clean up now“; fehlt die Zeile, kann das installierte
  Glut das noch nicht melden;
- „Clean up once a week at start“ (schaltet nur den Wochenlauf ab, nie den Knopf);
- „Save rules“ und „Clean up now“;
- „Orphaned worktrees waiting for you“ mit einem Papierkorb je Worktree (nur sichtbar, wenn es welche gibt).

Im Terminal: `axiomata-cli maintenance status` (zeigt nur) und `axiomata-cli maintenance clean [--dry-run]`.

## Häufige Handgriffe

1. **Jetzt aufräumen:** Settings → Storage → „Clean up now“. Die Rückfrage nennt, wie viel frei wird; Worktrees mit
   Änderungen bleiben.
2. **Eine Frist ändern:** die Zahl beim Posten ändern → „Save rules“. Erst danach „Clean up now“ — der Knopf räumt
   nach den gespeicherten Regeln und ist gesperrt, solange Änderungen offen sind.
3. **Einen verwaisten Worktree wegwerfen:** unter „Orphaned worktrees waiting for you“ den Papierkorb → bestätigen.
   Die ungesicherte Arbeit darin ist danach endgültig weg.

## Fallen

- Ein Worktree mit ungesicherter Arbeit wird **nie** von selbst gelöscht; er wartet, bis du ihn wegwirfst. Wer ihn
  behalten will, übernimmt die Arbeit vorher im Studio (siehe [Studio](studio.md)).
- Löschen eines Agenten im Studio nimmt seinen Worktree und Branch mit; hat er Änderungen, fragt die App vorher.
- Build-Ordner von Projekten (`target/`, `node_modules`) sind Sache der Projekte — „Storage“ zeigt sie nicht.
- Die Datenbank wird nur verdichtet, wenn es sich lohnt; ein Lauf ohne fällige Posten tut das trotzdem, falls nötig.

## Siehe auch

[Einstellungen](einstellungen.md) · [Kosten](kosten.md) · [Glut](glut.md)

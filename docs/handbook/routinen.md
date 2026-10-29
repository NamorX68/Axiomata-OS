# Routinen und ihr Zeitplan

Wie man einen Skill oder einen Prompt nach Zeitplan laufen lässt, wie das Intervall eingestellt wird und was passiert, wenn die App geschlossen ist.

## Was es ist

Eine **Routine** startet zu festen Zeiten einen Skill oder einen freien Prompt — etwa jeden Morgen den Mail-Digest.
Sie läuft auf dem Harness für Skills aus den Einstellungen, wenn du ihr kein eigenes Backend gibst. Jeder Lauf ist
ein Agenten-Zug wie ein Skill-Start von Hand und zählt gegen die Tagesgrenze.

## Wo die Knöpfe sind

- **Vorderseite der Kachel „Routines“:** Tabelle „Time | Routine | Status“. „Time“ zeigt, wie lange es bis zum
  nächsten Lauf ist; unter dem Namen steht das Ziel (`/skill` oder „prompt“). „Status“ ist „next“ (die nächste, mit
  einer abbrennenden Zündschnur), „queued“ oder „off“. Je Zeile: ✎ („Edit“), ✕ („Delete“) und der Schalter
  „on“/„off“.
- **Rückseite (Zahnrad):** „Show disabled routines“, „Sort“ („soonest first“ / „by name“) und das Formular „Add
  routine“.
- **Das Formular:** Name; Intervall „every N minutes“, „every N hours“, „daily“, „weekly“ oder „custom (cron)“; eine
  Vorschau wie „daily at 09:00“; Ziel „skill“ (Skillname) oder „prompt“ (Text); Backend („default (skill harness)“
  oder ein eigenes); „Add“.
- ⌘K findet Routinen (Enter zeigt die Kachel). Ist die Kachel offen, kann auch der Chat Routinen auflisten, anlegen
  und ändern.

## Häufige Handgriffe

1. **Routine anlegen:** Kachel umdrehen → „Add routine“ → Name, Intervall, Ziel → „Add“.
2. **Pausieren:** auf der Vorderseite den Schalter auf „off“.
3. **Uhrzeit ändern:** ✎ → Intervall anpassen → „Save“.

## Fallen

- **Nur bei laufender App.** Routinen feuert die App selbst; ist sie geschlossen, läuft nichts. Beim nächsten Start
  wird ein verpasster Termin als „Missed“ vermerkt und auf den nächsten verschoben — **nicht** nachgeholt.
- ✕ löscht die Routine **sofort**, ohne Rückfrage.
- „custom (cron)“ hat **sechs Felder, Sekunden zuerst**: `0 0 9 * * *` ist täglich 09:00 — nicht das übliche
  fünffeldrige Crontab-Format.
- Eine Routine auf einen Digest ersetzt nicht den ↻ der Kachel, ergänzt ihn: die Kachel zeigt jeweils den neuesten
  brauchbaren Lauf, egal wer ihn gestartet hat.

## Siehe auch

[Skills](skills.md) · [Verbindungen](verbindungen.md) · [Kosten](kosten.md)

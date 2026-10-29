# Kalender, Mail und Erinnerungen

Wie die Kacheln „Calendar“, „Mail“ und „Reminders“ an Apples Programme angebunden sind, wann sie sich auffrischen und was das kostet.

## Was es ist

Die drei Kacheln holen ihre Daten nicht selbst, sondern über einen **Digest-Skill** (`calendar-digest`,
`mail-digest`, `reminders-digest`), der über die MCP-Server `apple-reminders` und `apple-mail` liest und das Ergebnis
als Übersicht abgibt. Jede Kachel läuft einmal, wenn sie erscheint (App-Start oder frisch platziert), und dann nur
noch auf ↻ — **jeder Refresh ist ein echter Agenten-Zug** und kostet bei einem bezahlten Provider Geld. Anlegen,
Abhaken und Löschen gehen als stiller Einmal-Auftrag an den Agenten. Wiederkehrende Kalendertermine legt die App
direkt über macOS an, weil das Werkzeug keine Wiederholung kann.

## Wo die Knöpfe sind

- **Alle drei, am Kopf:** wann zuletzt gelaufen („never run“ oder relative Zeit) und ↻ („Refresh“). Rückseite:
  „Skill name“ — welcher Skill die Kachel füttert.
- **Calendar:** Filter „All calendars“, „+“ („New event“), Mini-Monat mit Punkten, Agenda mit ✕ je Termin.
  Formular: Titel, Kalender, Datum, „All day“, Zeiten, „Location (optional)…“, „Repeat“ („Does not repeat“,
  „Daily“, „Weekly“, „Monthly“, „Yearly“; dann „Every …“ und „until …“), „Create“. Rückseite: „Tage in der Agenda“
  (1–14), „Clock next to the calendar“ mit „Digital“ oder „Day arc“.
- **Mail** (nur lesen): „curated / last 3 days“, die Leiste „Today's mix“, die Abschnitte „Important“ und „By
  topic“; ein Klick öffnet die Zusammenfassung als Notiz in `Mail/`. Rückseite: „Topics (one per line)“ → „Save“ —
  gespeichert in der Vault-Datei `Mail/.topics.md`.
- **Reminders:** Listenwahl, „+“ („New reminder“), je Aufgabe Häkchen (erledigt), ✕ und Notizen. Formular: Titel,
  Liste, Fälligkeit, „Notes (optional)…“, „Create“.

## Häufige Handgriffe

1. **Auffrischen:** ↻ am Kopf der Kachel.
2. **Termin anlegen:** Calendar → „+“ → Formular → „Create“ (mit „Repeat“ für eine Serie).
3. **Regelmäßig auffrischen:** den Digest-Skill als [Routine](routinen.md) anlegen, etwa jeden Morgen.

## Fallen

- Keine Auffrischung im Hintergrund: was du außerhalb der App änderst, erscheint erst nach dem nächsten ↻ oder
  Routinenlauf.
- Calendar lädt nur diesen und den nächsten Monat; weiter blättert die Kachel nicht.
- Reminders kennt keine Ansicht „alle Listen“; die gewählte Liste bleibt gemerkt.
- Ist die Tagesgrenze erreicht, scheitert auch ein Refresh; die Kachel zeigt dann den letzten brauchbaren Stand mit
  einem Hinweis.
- Beim ersten Zugriff fragt macOS nach dem Recht auf Kalender, Erinnerungen oder Mail; nach einem neuen Build der
  installierten App kann es erneut fragen.

## Siehe auch

[Skills](skills.md) · [Routinen](routinen.md) · [Kosten](kosten.md) · [Provider](provider.md)

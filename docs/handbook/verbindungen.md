# Kalender, Mail und Erinnerungen

Wie die Kacheln „Kalender“, „Mail“ und „Erinnerungen“ an Apples Programme angebunden sind, wann sie sich auffrischen und was das kostet.

## Was es ist

Die drei Kacheln holen ihre Daten nicht selbst, sondern über einen **Digest-Skill** (`calendar-digest`,
`mail-digest`, `reminders-digest`), der über die MCP-Server `apple-reminders` und `apple-mail` liest und das Ergebnis
als Übersicht abgibt. Jede Kachel läuft einmal, wenn sie erscheint (App-Start oder frisch platziert), und dann nur
noch auf ↻ — **jeder Refresh ist ein echter Agenten-Zug** und kostet bei einem bezahlten Provider Geld. Anlegen,
Abhaken und Löschen gehen als stiller Einmal-Auftrag an den Agenten. Wiederkehrende Kalendertermine legt die App
direkt über macOS an, weil das Werkzeug keine Wiederholung kann.

## Wo die Knöpfe sind

- **Alle drei, am Kopf:** wann zuletzt gelaufen („nie gelaufen“ oder relative Zeit) und ↻ („Aktualisieren“). Rückseite:
  „Skill-Name“ — welcher Skill die Kachel füttert.
- **Kalender:** Filter „Alle Kalender“, „+“ („Neuer Termin“), Mini-Monat mit Punkten, Agenda mit ✕ je Termin. Formular:
  Titel, Kalender, Datum, „Ganztägig“, Zeiten, „Ort (optional)…“, „Wiederholen“ („Keine Wiederholung“, „Täglich“,
  „Wöchentlich“, „Monatlich“, „Jährlich“; dann „Alle …“ und „bis …“), „Anlegen“. Rückseite: „Tage in der Agenda“ (1–14),
  „Uhr neben dem Kalender“ mit „Digital“ oder „Tagesbogen“.
- **Mail** (nur lesen): „ausgewählt / letzte 3 Tage“, die Leiste „Heutige Mischung“, die Abschnitte „Wichtig“ und „Nach
  Thema“; ein Klick öffnet die Zusammenfassung als Notiz in `Mail/`. Rückseite: „Themen (eins pro Zeile)“ → „Speichern“
  — gespeichert in der Vault-Datei `Mail/.topics.md`.
- **Erinnerungen:** Listenwahl, „+“ („Neue Erinnerung“), je Aufgabe Häkchen (erledigt), ✕ und Notizen. Formular: Titel,
  Liste, Fälligkeit, „Notizen (optional)…“, „Anlegen“.

## Häufige Handgriffe

1. **Auffrischen:** ↻ am Kopf der Kachel.
2. **Termin anlegen:** Kalender → „+“ → Formular → „Anlegen“ (mit „Wiederholen“ für eine Serie).
3. **Regelmäßig auffrischen:** den Digest-Skill als [Routine](routinen.md) anlegen, etwa jeden Morgen.

## Fallen

- Keine Auffrischung im Hintergrund: was du außerhalb der App änderst, erscheint erst nach dem nächsten ↻ oder
  Routinenlauf.
- Der Kalender lädt nur diesen und den nächsten Monat; weiter blättert die Kachel nicht.
- Erinnerungen kennt keine Ansicht „alle Listen“; die gewählte Liste bleibt gemerkt.
- Ist die Tagesgrenze erreicht, scheitert auch ein Refresh; die Kachel zeigt dann den letzten brauchbaren Stand mit
  einem Hinweis.
- Beim ersten Zugriff fragt macOS nach dem Recht auf Kalender, Erinnerungen oder Mail; nach einem neuen Build der
  installierten App kann es erneut fragen.

## Siehe auch

[Skills](skills.md) · [Routinen](routinen.md) · [Kosten](kosten.md) · [Provider](provider.md)

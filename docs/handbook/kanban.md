# Kanban

Wie das Kanban-Board funktioniert: Boards, Spalten mit den festen Spalten Proposal und Review, Kartenfelder, die Knöpfe einer Karte und was Agenten auf dem Board dürfen.

## Was es ist

Das Board ist Axiomatas Aufgabenliste für alles — eigene Punkte ebenso wie die Karten, die Agenten im Studio
bearbeiten. Es steht in der Datenbank der App; zusätzlich schreibt die App jedes Board als Notiz nach
`Kanban/<id>-<name>.md` in den Vault, damit Obsidian, die Suche und die Agenten es sehen. Kanban ist **keine
Kachel**, sondern eine große Ansicht.

Jedes Board hat neben deinen Spalten zwei feste: **Proposal** (Vorschläge von Agenten, wartet auf dein Ja; leer
ausgeblendet) und **Review** (fertige Arbeit wartet auf Prüfung und Übernahme). Beide lassen sich nicht entfernen.

## Wo die Knöpfe sind

- **Öffnen:** App-Ring oder Dock „Kanban“, ⌘K „Kanban öffnen“; es öffnet das zuletzt benutzte Board.
- **Werkzeugleiste:** „Wähle ein Board“, „Karten filtern…“, Label-Chips, „Archivierte Karten zeigen“ / „… ausblenden“,
  „Board neu laden“, „Spalte hinzufügen“, „Boards & Ansicht“ („Neues Board…“, Name direkt umbenennen, ✕ „Board löschen“,
  „Farbstreifen nach dem ersten Label“).
- **Spalte:** Menü mit Rolle („offen“ / „in Arbeit“ / „erledigt“), „Umbenennen“, „Entfernen“; unten „+ Karte“.
- **Karte öffnen:** Felder Titel, Text, Labels, „Fällig“, „Zugewiesen an“; der Abschnitt „Agenten-Flow“ (Art, Level,
  Rolle, Abhängigkeiten „Braucht zuerst“); „Verlauf“ mit „Schreibe eine Notiz…“; bei Studio-Arbeit „Verbrauch“.
- **Knöpfe einer Karte** (je nach Zustand): „Starten…“, „Prüfung starten…“, „Übernehmen…“, „Im Flow öffnen“ bzw. „Im
  Flow übernehmen…“, „Freigeben“, „Vorschlag annehmen“, „Wieder öffnen“, „Absagen“, „Archivieren“ / „Wiederherstellen“,
  „Löschen“.

## Häufige Handgriffe

1. **Karte anlegen:** in der Spalte „+ Karte“ → Titel → Enter.
2. **Karte von Agenten erledigen lassen:** Karte öffnen → „Starten…“ → Projekt → „Als Plan starten“ (wird ein Plan im
   [Flow](flow.md), bis 5 $). Eine Karte, die schon Schritt eines Plans ist, startet mit „Starten…“ auf einer Engine.
3. **Vorschlag eines Agenten annehmen:** Karte in „Proposal“ öffnen → „Vorschlag annehmen“ (oder „Absagen“).

## Fallen

- **Agenten legen Karten nur in „Proposal“ an.** Freigeben, Review abschließen, Übernehmen, Archivieren und Löschen
  sind deine Schritte; der Chat verschiebt nur einfache Karten.
- Eine neue Karte gehört in die erste normale offene Spalte, nicht in „Proposal“ — dort wartet sie auf eine
  Freigabe, mit der niemand rechnet.
- Die Notizen in `Kanban/` sind nur ein Spiegel: was du dort änderst, liest die App **nie** zurück und überschreibt
  es bei der nächsten Änderung am Board.
- „Übernehmen…“ übernimmt die Arbeit in den Branch des Projekts, pusht aber nichts.
- „Freigeben“ gibt eine laufende Karte frei: sie wartet wieder offen, die Sitzung verliert ihren Zugang.
- Eine Spalte mit Karten fragt beim Entfernen, wohin die Karten sollen.

## Siehe auch

[Flow](flow.md) · [Studio](studio.md) · [Chat](chat.md) · [Notizen](notizen.md)

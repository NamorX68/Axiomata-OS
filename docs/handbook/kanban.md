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

- **Öffnen:** App-Ring oder Dock „Kanban“, ⌘K „Open Kanban“; es öffnet das zuletzt benutzte Board.
- **Werkzeugleiste:** „Choose a board“, „Filter cards …“, Label-Chips, „Show/Hide archived cards“, „Reload the
  board“, „Add a column“, „Boards & view“ („New board …“, Name direkt umbenennen, ✕ „Delete the board“, „Colour
  stripe by the first label“).
- **Spalte:** Menü mit Rolle („open“ / „in progress“ / „done“), „Rename“, „Remove“; unten „+ Card“.
- **Karte öffnen:** Felder Titel, Text, Labels, „Due“, „Assigned to“; der Abschnitt „Ablauf“ (Art, Level, Rolle,
  Abhängigkeiten „Braucht zuerst“); „Verlauf“ mit „Write a note …“; bei Studio-Arbeit „Verbrauch“.
- **Knöpfe einer Karte** (je nach Zustand): „Start …“, „Start review …“, „Take over …“, „Open in the Flow“ bzw.
  „Take over in the Flow …“, „Release“, „Accept proposal“, „Reopen“, „Absagen“, „Archive“ / „Restore“, „Delete“.

## Häufige Handgriffe

1. **Karte anlegen:** in der Spalte „+ Card“ → Titel → Enter.
2. **Karte von Agenten erledigen lassen:** Karte öffnen → „Start …“ → Projekt → „Start as plan“ (wird ein Plan im
   [Flow](flow.md), bis 5 $). Eine Karte, die schon Schritt eines Plans ist, startet mit „Start …“ auf einer Engine.
3. **Vorschlag eines Agenten annehmen:** Karte in „Proposal“ öffnen → „Accept proposal“ (oder „Absagen“).

## Fallen

- **Agenten legen Karten nur in „Proposal“ an.** Freigeben, Review abschließen, Übernehmen, Archivieren und Löschen
  sind deine Schritte; der Chat verschiebt nur einfache Karten.
- Eine neue Karte gehört in die erste normale offene Spalte, nicht in „Proposal“ — dort wartet sie auf eine
  Freigabe, mit der niemand rechnet.
- Die Notizen in `Kanban/` sind nur ein Spiegel: was du dort änderst, liest die App **nie** zurück und überschreibt
  es bei der nächsten Änderung am Board.
- „Take over …“ übernimmt die Arbeit in den Branch des Projekts, pusht aber nichts.
- „Release“ gibt eine laufende Karte frei: sie wartet wieder offen, die Sitzung verliert ihren Zugang.
- Eine Spalte mit Karten fragt beim Entfernen, wohin die Karten sollen.

## Siehe auch

[Flow](flow.md) · [Studio](studio.md) · [Chat](chat.md) · [Notizen](notizen.md)
